//! Motion vector prediction and direct mode derivation.
//!
//! Contains MV prediction (median, directional), spatial/temporal direct mode,
//! MV neighbor lookups, skip MV derivation, and weighted prediction context.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::dpb::{DecodedPicture, Motion, PictureStructure, ViewSource};
use crate::pixel::Pixel;
use crate::residual::OFFSET_TO_BLOCK;
use crate::slice::PredWeightTable;

pub(crate) type DirectMvResult = ([i16; 2], [i16; 2], i8, i8, bool, bool);

/// MBAFF context for neighbor derivation. Passed to MV prediction functions.
#[derive(Clone, Copy)]
pub(crate) struct MbaffCtx<'a> {
    pub mbaff: bool,
    pub mb_field_decoding: &'a [bool],
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn predict_mv_sub(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mb_idx: usize,
    mb_width: usize,
    px: usize,
    py: usize,
    spw: usize,
    _sph: usize,
    ref_idx: i8,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mctx: MbaffCtx,
) -> (i16, i16) {
    let a = get_mv_neighbor_left_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        py,
        px,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    );
    let b = get_mv_neighbor_above_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        py,
        px,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    );
    let c = get_mv_neighbor_above_right_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        py,
        px,
        spw,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    )
    .or_else(|| {
        get_mv_neighbor_above_left_mbaff(
            mv_store_l0,
            ref_idx_store_l0,
            mb_idx,
            mb_width,
            py,
            px,
            mb_slice_id,
            cur_slice_id,
            mctx.mbaff,
            mctx.mb_field_decoding,
        )
    });

    // match_count directional logic (same as predict_mv)
    let ref_a = a.map(|(_, r)| r).unwrap_or(-1);
    let ref_b = b.map(|(_, r)| r).unwrap_or(-1);
    let ref_c = c.map(|(_, r)| r).unwrap_or(-1);
    let match_count =
        (ref_a == ref_idx) as u8 + (ref_b == ref_idx) as u8 + (ref_c == ref_idx) as u8;

    if match_count == 1 {
        if ref_a == ref_idx {
            if let Some((mv, _)) = a {
                return (mv[0], mv[1]);
            }
        }
        if ref_b == ref_idx {
            if let Some((mv, _)) = b {
                return (mv[0], mv[1]);
            }
        }
        if ref_c == ref_idx {
            if let Some((mv, _)) = c {
                return (mv[0], mv[1]);
            }
        }
    }

    if let (None, None, Some((mv, _))) = (b, c, a) {
        return (mv[0], mv[1]);
    }

    let mv_a = a.map(|(mv, _)| mv).unwrap_or([0, 0]);
    let mv_b = b.map(|(mv, _)| mv).unwrap_or([0, 0]);
    let mv_c = c.map(|(mv, _)| mv).unwrap_or([0, 0]);

    let mut xs = [mv_a[0], mv_b[0], mv_c[0]];
    let mut ys = [mv_a[1], mv_b[1], mv_c[1]];
    xs.sort();
    ys.sort();
    (xs[1], ys[1])
}

/// Safely index a ref pic list, clamping out-of-range indices to the last entry.
/// Returns `None` if the list is empty (malformed bitstream).
#[inline]
pub(crate) fn ref_pic_safe(list: &[Arc<DecodedPicture>], idx: i8) -> Option<&Arc<DecodedPicture>> {
    if list.is_empty() {
        return None;
    }
    Some(&list[(idx.max(0) as usize).min(list.len() - 1)])
}

/// Bundles weighted prediction parameters for a slice, avoiding long argument lists.
pub(crate) struct WeightContext<'a> {
    pub(crate) use_weight: u8,
    pub(crate) wt: Option<&'a PredWeightTable>,
    pub(crate) implicit_weights: &'a [Vec<i32>],
    /// MBAFF field MBs: implicit weights per MB parity over field ref_idx
    pub(crate) implicit_weights_field: &'a [Vec<Vec<i32>>; 2],
}

/// Weighted prediction for one macroblock: `field` is the MB's parity
/// (0 = top) when it is an MBAFF field MB, whose ref_idx address fields.
#[derive(Clone, Copy)]
pub(crate) struct MbWeights<'a> {
    ctx: &'a WeightContext<'a>,
    field: Option<usize>,
}

impl<'a> WeightContext<'a> {
    /// Weighting for macroblock `mb_idx`; `field_mb` is true for an MBAFF
    /// field MB.
    pub(crate) fn mb(&'a self, field_mb: bool, mb_idx: usize) -> MbWeights<'a> {
        MbWeights {
            ctx: self,
            field: field_mb.then_some(mb_idx % 2),
        }
    }
}

impl MbWeights<'_> {
    /// Explicit weight table index: a field MB's ref_idx addresses fields,
    /// two per table entry (spec 8.4.2.3: refIdxL0WP = refIdxL0 >> 1).
    #[inline]
    fn wp_idx(&self, ref_idx: usize) -> usize {
        if self.field.is_some() {
            ref_idx >> 1
        } else {
            ref_idx
        }
    }

    /// Apply weighted uni-prediction in place. No-op if use_weight != 1.
    #[inline]
    pub(crate) fn apply_uni<P: Pixel>(
        &self,
        pred: &mut [P],
        list: usize,
        ref_idx: usize,
        is_chroma: bool,
        chroma_comp: usize,
    ) {
        self.ctx
            .apply_uni(pred, list, self.wp_idx(ref_idx), is_chroma, chroma_comp);
    }

    /// Weighted bi-prediction of this MB (plain average when unweighted).
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub(crate) fn apply_bi<P: Pixel>(
        &self,
        pred_l0: &[P],
        pred_l1: &[P],
        output: &mut [P],
        ref_idx_l0: usize,
        ref_idx_l1: usize,
        is_chroma: bool,
        chroma_comp: usize,
    ) {
        match (self.ctx.use_weight, self.field) {
            (2, Some(p)) => {
                let w0 = self.ctx.implicit_weights_field[p]
                    .get(ref_idx_l0)
                    .and_then(|row| row.get(ref_idx_l1));
                match w0 {
                    Some(&w0) => P::weighted_bi_implicit(pred_l0, pred_l1, output, w0, 64 - w0),
                    None => P::bi_pred_avg(pred_l0, pred_l1, output),
                }
            }
            _ => self.ctx.apply_bi(
                pred_l0,
                pred_l1,
                output,
                self.wp_idx(ref_idx_l0),
                self.wp_idx(ref_idx_l1),
                is_chroma,
                chroma_comp,
            ),
        }
    }
}

impl WeightContext<'_> {
    fn apply_uni<P: Pixel>(
        &self,
        pred: &mut [P],
        list: usize,
        ref_idx: usize,
        is_chroma: bool,
        chroma_comp: usize,
    ) {
        if self.use_weight != 1 {
            return;
        }
        let wt = match self.wt {
            Some(w) => w,
            None => return,
        };
        let refs = if list == 0 { &wt.l0 } else { &wt.l1 };
        if ref_idx >= refs.len() {
            return;
        }
        let rw = &refs[ref_idx];
        if is_chroma {
            P::weighted_uni(
                pred,
                wt.chroma_log2_weight_denom,
                rw.chroma_weight[chroma_comp],
                rw.chroma_offset[chroma_comp],
            );
        } else {
            P::weighted_uni(
                pred,
                wt.luma_log2_weight_denom,
                rw.luma_weight,
                rw.luma_offset,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_bi<P: Pixel>(
        &self,
        pred_l0: &[P],
        pred_l1: &[P],
        output: &mut [P],
        ref_idx_l0: usize,
        ref_idx_l1: usize,
        is_chroma: bool,
        chroma_comp: usize,
    ) {
        match self.use_weight {
            1 => {
                let wt = match self.wt {
                    Some(w) => w,
                    None => {
                        P::bi_pred_avg(pred_l0, pred_l1, output);
                        return;
                    }
                };
                let rw0 = wt.l0.get(ref_idx_l0);
                let rw1 = wt.l1.get(ref_idx_l1);
                match (rw0, rw1) {
                    (Some(w0), Some(w1)) if is_chroma => {
                        P::weighted_bi(
                            pred_l0,
                            pred_l1,
                            output,
                            wt.chroma_log2_weight_denom,
                            w0.chroma_weight[chroma_comp],
                            w0.chroma_offset[chroma_comp],
                            w1.chroma_weight[chroma_comp],
                            w1.chroma_offset[chroma_comp],
                        );
                    }
                    (Some(w0), Some(w1)) => {
                        P::weighted_bi(
                            pred_l0,
                            pred_l1,
                            output,
                            wt.luma_log2_weight_denom,
                            w0.luma_weight,
                            w0.luma_offset,
                            w1.luma_weight,
                            w1.luma_offset,
                        );
                    }
                    _ => P::bi_pred_avg(pred_l0, pred_l1, output),
                }
            }
            2 => {
                if ref_idx_l0 < self.implicit_weights.len()
                    && ref_idx_l1 < self.implicit_weights[ref_idx_l0].len()
                {
                    let w0 = self.implicit_weights[ref_idx_l0][ref_idx_l1];
                    P::weighted_bi_implicit(pred_l0, pred_l1, output, w0, 64 - w0);
                } else {
                    P::bi_pred_avg(pred_l0, pred_l1, output);
                }
            }
            _ => P::bi_pred_avg(pred_l0, pred_l1, output),
        }
    }
}

/// Motion vector prediction for P_Skip macroblocks (spec 8.4.1.1).
///
/// Returns (0, 0) if either neighbor A (left) or B (above) is unavailable
/// or has ref_idx=0 with zero MV. Otherwise uses the standard median predictor.
pub(crate) fn predict_mv_skip(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mb_idx: usize,
    mb_width: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mctx: MbaffCtx,
) -> (i16, i16) {
    let a = get_mv_neighbor_left_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        0,
        0,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    );
    let b = get_mv_neighbor_above_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        0,
        0,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    );

    // Spec 8.4.1.1: if A is unavailable or (refA==0 && mvA==(0,0)), OR
    // if B is unavailable or (refB==0 && mvB==(0,0)), then skip MV = (0,0).
    let a_zero = match a {
        None => true,
        Some((mv, ri)) => ri == 0 && mv[0] == 0 && mv[1] == 0,
    };
    let b_zero = match b {
        None => true,
        Some((mv, ri)) => ri == 0 && mv[0] == 0 && mv[1] == 0,
    };
    if a_zero || b_zero {
        return (0, 0);
    }

    predict_mv(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        0,
        16,
        16,
        0,
        mb_slice_id,
        cur_slice_id,
        mctx,
    )
}

/// Derive spatial direct mode MVs with per-4x4-block co-located check.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn derive_spatial_direct_blk(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mv_store_l1: &[[i16; 2]],
    ref_idx_store_l1: &[i8],
    mb_idx: usize,
    mb_width: usize,
    col_pic: Option<&DecodedPicture>,
    col_blk: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    direct_8x8_inference_flag: bool,
    mctx: MbaffCtx,
    cur_poc: i32,
    cur_bottom: bool,
) -> DirectMvResult {
    let base = derive_spatial_direct_base(
        [
            (mv_store_l0, ref_idx_store_l0),
            (mv_store_l1, ref_idx_store_l1),
        ],
        mb_idx,
        mb_width,
        mb_slice_id,
        cur_slice_id,
        mctx,
    );
    apply_spatial_direct_col_zero(
        base,
        col_pic,
        mb_idx,
        col_blk,
        direct_8x8_inference_flag,
        mctx,
        cur_poc,
        cur_bottom,
    )
}

/// How the current picture and its co-located picture are coded (spec
/// Table 8-6), for mapping references and vertical motion.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColKind {
    /// Same structure, or an MBAFF frame (per-MB frame/field handled by
    /// `colocated_pos`)
    Same,
    /// Frame picture, co-located field of a field pair (Fld_To_Frm)
    FrameFromField,
    /// Field picture, co-located frame (Frm_To_Fld, or a field MB pair of
    /// an MBAFF frame)
    FieldFromFrame,
    /// Field MB of an MBAFF frame, co-located field of a field pair (the
    /// field of the MB's parity, no vertical scaling)
    MbaffFieldFromField,
    /// Field picture, co-located field MB pair of an MBAFF frame (the MB of
    /// the current parity, no vertical scaling)
    FieldFromMbaffField,
}

/// The co-located picture whose motion direct mode reads, the index of
/// the co-located 4x4 block in it, and how it relates to the current
/// picture. `col_pic` is RefPicList1[0]; for a PAFF view the motion lives
/// in the picture(s) behind it (spec 8.4.1.2.1, Table 8-6):
/// - a frame picture with a field pair uses the field closer in POC, MB
///   `W * (mb_y / 2) + mb_x`, block row `2 * (mb_y % 2) + y8`;
/// - a field picture with a frame uses MB `2 * W * mb_y + mb_x + W * y8`,
///   block row `2 * y8` (in an MBAFF frame: the pair's MB of the current
///   parity when the pair is field coded, else that MB of the pair).
#[allow(clippy::too_many_arguments)]
pub(crate) fn locate_colocated<'a>(
    col_pic: &'a DecodedPicture,
    mb_idx: usize,
    blk: usize,
    direct_8x8_inference_flag: bool,
    mctx: MbaffCtx,
    cur_poc: i32,
    cur_bottom: bool,
) -> (&'a DecodedPicture, usize, ColKind) {
    let w = (col_pic.mb_width as usize).max(1);
    let (mb_x, mb_y) = (mb_idx % w, mb_idx / w);
    let (x8, y8) = ((blk / 4) & 1, blk / 8);
    match &col_pic.view_source {
        Some(ViewSource::Pair(fields)) if mctx.mbaff => {
            // MBAFF frame over a field pair: the fields hold the motion in
            // MB pair positions (field MB row = pair row)
            let pair = mb_idx / 2;
            let bottom = mb_idx % 2;
            if mctx.mb_field_decoding[pair] {
                const INFERENCE_MAP: [usize; 4] = [0, 5, 10, 15];
                let b = if direct_8x8_inference_flag {
                    INFERENCE_MAP[blk / 4]
                } else {
                    blk
                };
                return (&fields[bottom], pair * 16 + b, ColKind::MbaffFieldFromField);
            }
            let [top, bot] = fields;
            let parity = ((top.pic_order_cnt as i64 - cur_poc as i64).abs()
                >= (bot.pic_order_cnt as i64 - cur_poc as i64).abs())
                as usize;
            let row = 2 * bottom + y8;
            (
                &fields[parity],
                pair * 16 + OFFSET_TO_BLOCK[row][3 * x8],
                ColKind::FrameFromField,
            )
        }
        Some(ViewSource::Pair(fields)) => {
            let [top, bottom] = fields;
            let parity = ((top.pic_order_cnt as i64 - cur_poc as i64).abs()
                >= (bottom.pic_order_cnt as i64 - cur_poc as i64).abs())
                as usize;
            let col_mb = w * (mb_y / 2) + mb_x;
            let row = 2 * (mb_y % 2) + y8;
            (
                &fields[parity],
                col_mb * 16 + OFFSET_TO_BLOCK[row][3 * x8],
                ColKind::FrameFromField,
            )
        }
        Some(ViewSource::FieldOf(frame)) => {
            let mb_field = &frame.motion().mb_field;
            if !mb_field.is_empty() {
                // MBAFF frame: the field MB pair at the current MB's place
                let pair = mb_idx;
                if mb_field.get(pair).copied().unwrap_or(false) {
                    let col_mb = 2 * pair + cur_bottom as usize;
                    const INFERENCE_MAP: [usize; 4] = [0, 5, 10, 15];
                    let b = if direct_8x8_inference_flag {
                        INFERENCE_MAP[blk / 4]
                    } else {
                        blk
                    };
                    return (frame, col_mb * 16 + b, ColKind::FieldFromMbaffField);
                }
                let col_mb = 2 * pair + y8;
                return (
                    frame,
                    col_mb * 16 + OFFSET_TO_BLOCK[2 * y8][3 * x8],
                    ColKind::FieldFromFrame,
                );
            }
            let col_mb = 2 * w * mb_y + mb_x + w * y8;
            (
                frame,
                col_mb * 16 + OFFSET_TO_BLOCK[2 * y8][3 * x8],
                ColKind::FieldFromFrame,
            )
        }
        _ => (
            col_pic,
            colocated_pos(
                col_pic.field_poc,
                col_pic.motion(),
                mb_idx,
                blk,
                direct_8x8_inference_flag,
                mctx,
                cur_poc,
            ),
            ColKind::Same,
        ),
    }
}

/// Index into the co-located picture's per-4x4 motion arrays for 4x4 block
/// `blk` of `mb_idx` (spec 8.4.1.2.1, Table 8-6).
///
/// With `direct_8x8_inference_flag` each 8x8 uses its outer corner block. In
/// MBAFF, when the co-located pair and the current MB differ in frame/field
/// coding (inference is then mandatory, spec 7.4.2.1.1):
/// - frame MB, field pair: the co-located MB is the pair's field closer in
///   POC to the current picture (ties go to the bottom field), at block row
///   `2 * (mb_idx % 2) + y8`;
/// - field MB, frame pair: the upper 8x8s use the pair's top MB and the
///   lower ones its bottom MB, at block row `2 * y8`.
pub(crate) fn colocated_pos(
    col_field_poc: [i32; 2],
    col_motion: &Motion,
    mb_idx: usize,
    blk: usize,
    direct_8x8_inference_flag: bool,
    mctx: MbaffCtx,
    cur_poc: i32,
) -> usize {
    const INFERENCE_MAP: [usize; 4] = [0, 5, 10, 15];
    let pair = mb_idx / 2;
    if mctx.mbaff && direct_8x8_inference_flag && pair < col_motion.mb_field.len() {
        let cur_field = mctx.mb_field_decoding[pair];
        if cur_field != col_motion.mb_field[pair] {
            let (x8, y8) = ((blk / 4) & 1, blk / 8);
            let (col_mb, row) = if cur_field {
                (pair * 2 + y8, 2 * y8)
            } else {
                let [top, bottom] = col_field_poc;
                let parity = ((top as i64 - cur_poc as i64).abs()
                    >= (bottom as i64 - cur_poc as i64).abs())
                    as usize;
                (pair * 2 + parity, 2 * (mb_idx % 2) + y8)
            };
            return col_mb * 16 + OFFSET_TO_BLOCK[row][3 * x8];
        }
    }
    let effective_blk = if direct_8x8_inference_flag {
        INFERENCE_MAP[blk / 4]
    } else {
        blk
    };
    mb_idx * 16 + effective_blk
}

/// Derive the neighbor-based spatial direct result before colocated zero-MV refinement.
///
/// The spatial neighbor set is addressed at the macroblock origin in this decoder's
/// direct-mode path, so it is invariant for all 8x8 representatives within the MB.
pub(crate) fn derive_spatial_direct_base(
    stores: [(&[[i16; 2]], &[i8]); 2],
    mb_idx: usize,
    mb_width: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mctx: MbaffCtx,
) -> DirectMvResult {
    let mut ref_idx = [-1i8; 2];
    let mut mv = [[0i16; 2]; 2];
    let mut pred_flag = [false; 2];

    // For each list, find min-positive ref_idx from neighbors and derive MV
    for list in 0..2 {
        let (mv_s, ref_s) = stores[list];

        let a = get_mv_neighbor_left_mbaff(
            mv_s,
            ref_s,
            mb_idx,
            mb_width,
            0,
            0,
            mb_slice_id,
            cur_slice_id,
            mctx.mbaff,
            mctx.mb_field_decoding,
        );
        let b = get_mv_neighbor_above_mbaff(
            mv_s,
            ref_s,
            mb_idx,
            mb_width,
            0,
            0,
            mb_slice_id,
            cur_slice_id,
            mctx.mbaff,
            mctx.mb_field_decoding,
        );
        let c = get_mv_neighbor_above_right_mbaff(
            mv_s,
            ref_s,
            mb_idx,
            mb_width,
            0,
            0,
            16,
            mb_slice_id,
            cur_slice_id,
            mctx.mbaff,
            mctx.mb_field_decoding,
        )
        .or_else(|| {
            get_mv_neighbor_above_left_mbaff(
                mv_s,
                ref_s,
                mb_idx,
                mb_width,
                0,
                0,
                mb_slice_id,
                cur_slice_id,
                mctx.mbaff,
                mctx.mb_field_decoding,
            )
        });

        let ref_a = a.map(|(_, r)| r).unwrap_or(-1);
        let ref_b = b.map(|(_, r)| r).unwrap_or(-1);
        let ref_c = c.map(|(_, r)| r).unwrap_or(-1);

        // Min-positive rule: minimum of valid (>= 0) ref indices
        let min_ref = [ref_a, ref_b, ref_c]
            .iter()
            .filter(|&&r| r >= 0)
            .min()
            .copied()
            .unwrap_or(-1);

        ref_idx[list] = min_ref;

        if min_ref >= 0 {
            pred_flag[list] = true;

            // Median MV prediction with match_count directional logic
            let match_count =
                (ref_a == min_ref) as u8 + (ref_b == min_ref) as u8 + (ref_c == min_ref) as u8;

            if match_count == 1 {
                // Use the single matching neighbor's MV
                if ref_a == min_ref {
                    if let Some((m, _)) = a {
                        mv[list] = m;
                        continue;
                    }
                }
                if ref_b == min_ref {
                    if let Some((m, _)) = b {
                        mv[list] = m;
                        continue;
                    }
                }
                if ref_c == min_ref {
                    if let Some((m, _)) = c {
                        mv[list] = m;
                        continue;
                    }
                }
            }

            // match_count >= 2 or fallback: median
            if let (None, None, Some((m, _))) = (b, c, a) {
                mv[list] = m;
                continue;
            }

            let mv_a = a.map(|(m, _)| m).unwrap_or([0, 0]);
            let mv_b = b.map(|(m, _)| m).unwrap_or([0, 0]);
            let mv_c = c.map(|(m, _)| m).unwrap_or([0, 0]);
            let mut xs = [mv_a[0], mv_b[0], mv_c[0]];
            let mut ys = [mv_a[1], mv_b[1], mv_c[1]];
            xs.sort();
            ys.sort();
            mv[list] = [xs[1], ys[1]];
        }
    }

    // If both refs invalid, default to ref_idx=0 for both lists (bi-prediction)
    if ref_idx[0] < 0 && ref_idx[1] < 0 {
        ref_idx = [0, 0];
        pred_flag = [true, true];
        mv = [[0, 0], [0, 0]];
    }

    (
        mv[0],
        mv[1],
        ref_idx[0],
        ref_idx[1],
        pred_flag[0],
        pred_flag[1],
    )
}

/// Apply spatial direct colocated zero-MV refinement to an already-derived base result.
///
/// Only the colocated block lookup depends on `col_blk`; the spatial median/ref result
/// is unchanged across 8x8 representatives within the same macroblock.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_spatial_direct_col_zero(
    base: DirectMvResult,
    col_pic: Option<&DecodedPicture>,
    mb_idx: usize,
    col_blk: usize,
    direct_8x8_inference_flag: bool,
    mctx: MbaffCtx,
    cur_poc: i32,
    cur_bottom: bool,
) -> DirectMvResult {
    let (mut mv_l0, mut mv_l1, ref_idx_l0, ref_idx_l1, pred_l0, pred_l1) = base;

    // Co-located zero-MV refinement (spec 8.4.1.2.2):
    // Derive mvCol/refIdxCol from the co-located partition in ColPic (= RefPicList1[0]):
    //   - If co-located PredFlagL0 = 1: use L0 MV/ref
    //   - If co-located PredFlagL0 = 0 (L1-only, e.g. B_L1_16x16): use L1 MV/ref
    // Then colZeroFlag = 1 if refIdxCol maps to the same picture as
    // RefPicList0[0] and |mvCol| <= 1 in both components.
    // When colZeroFlag is set, zero out spatial MVs for lists where ref_idx == 0.
    // When direct_8x8_inference_flag is set, use the representative 4x4 block
    // per 8x8 group (same mapping as temporal direct).
    if let Some(col_pic) = col_pic {
        let (col, col_pos, _) = locate_colocated(
            col_pic,
            mb_idx,
            col_blk,
            direct_8x8_inference_flag,
            mctx,
            cur_poc,
            cur_bottom,
        );
        // Co-located motion data is only final once the whole picture is.
        let col_motion = col.motion();
        if col_pos < col_motion.ref_idx_l0.len() && !col.is_intra {
            let col_ref_l0 = col_motion.ref_idx_l0[col_pos];
            // Determine which co-located MV to use for the zero check:
            // If co-located has L0 prediction (ref_idx_l0 >= 0): use L0 MV/ref
            // If co-located is L1-only (ref_idx_l0 < 0): use L1 MV/ref per spec
            let col_zero = if col_ref_l0 == 0 {
                let col_mv = if col_pos < col_motion.mv_l0.len() {
                    col_motion.mv_l0[col_pos]
                } else {
                    [0, 0]
                };
                col_mv[0].abs() <= 1 && col_mv[1].abs() <= 1
            } else if col_ref_l0 < 0
                && col_pos < col_motion.ref_idx_l1.len()
                && col_motion.ref_idx_l1[col_pos] == 0
            {
                let col_mv_l1 = if col_pos < col_motion.mv_l1.len() {
                    col_motion.mv_l1[col_pos]
                } else {
                    [0, 0]
                };
                col_mv_l1[0].abs() <= 1 && col_mv_l1[1].abs() <= 1
            } else {
                false
            };

            if col_zero {
                if ref_idx_l0 == 0 {
                    mv_l0 = [0, 0];
                }
                if ref_idx_l1 == 0 {
                    mv_l1 = [0, 0];
                }
            }
        }
    }

    (mv_l0, mv_l1, ref_idx_l0, ref_idx_l1, pred_l0, pred_l1)
}

/// Derive temporal direct mode MVs for 4x4 block `blk` of `mb_idx` (spec
/// 8.4.1.2.3): the co-located block's motion, L0 or else L1, scaled by POC
/// distance. `current_poc` is the picture's POC and field POCs; `col_pic` is
/// RefPicList1[0].
///
/// In an MBAFF field MB the reference indices address fields and the
/// distances use field POCs: the MB's own parity, the same-parity field of
/// `col_pic`, and the referenced field. When the co-located MB is coded the
/// other way (frame vs field) its vertical MV is doubled (field to frame) or
/// halved, and a frame reference maps to the same-parity field (as FFmpeg's
/// `pred_temp_direct_motion`).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn derive_temporal_direct_blk(
    col_pic: &DecodedPicture,
    ref_pic_list_l0: &[Arc<DecodedPicture>],
    long_term_l0: &[bool],
    (current_poc, current_field_poc): (i32, [i32; 2]),
    mb_idx: usize,
    blk: usize,
    direct_8x8_inference_flag: bool,
    mctx: MbaffCtx,
    cur_bottom: bool,
) -> ([i16; 2], [i16; 2], i8, i8, bool, bool) {
    const INTRA: ([i16; 2], [i16; 2], i8, i8, bool, bool) = ([0, 0], [0, 0], 0, 0, true, true);
    let (col, col_pos, kind) = locate_colocated(
        col_pic,
        mb_idx,
        blk,
        direct_8x8_inference_flag,
        mctx,
        current_poc,
        cur_bottom,
    );
    let col_motion = col.motion();
    if col.is_intra || col_pos >= col_motion.ref_idx_l0.len() {
        return INTRA;
    }
    // mvCol/refIdxCol: L0 motion, or L1 when the co-located block has no L0
    let (col_mv, col_ref, col_ref_poc) = if col_motion.ref_idx_l0[col_pos] >= 0 {
        (
            col_motion.mv_l0[col_pos],
            col_motion.ref_idx_l0[col_pos],
            col_motion.ref_poc_l0[col_pos],
        )
    } else if col_motion.ref_idx_l1.get(col_pos).is_some_and(|&r| r >= 0) {
        (
            col_motion.mv_l1[col_pos],
            col_motion.ref_idx_l1[col_pos],
            col_motion.ref_poc_l1.get(col_pos).copied().unwrap_or(-1),
        )
    } else {
        return INTRA;
    };

    // The current L0 entry of the picture the co-located block referenced;
    // a field picture's references are keyed by POC and parity
    let col_is_field = col.structure != PictureStructure::Frame;
    let frame_idx = ref_pic_list_l0
        .iter()
        .position(|p| {
            if col_is_field {
                field_ref_key(p) == col_ref_poc
            } else {
                p.pic_order_cnt == col_ref_poc
            }
        })
        .unwrap_or(0);
    let (cur_field, col_field) = match kind {
        ColKind::MbaffFieldFromField | ColKind::FieldFromMbaffField => (true, true),
        ColKind::FrameFromField => (false, true),
        ColKind::FieldFromFrame => (true, false),
        ColKind::Same => (
            mctx.mbaff && mctx.mb_field_decoding[mb_idx / 2],
            mctx.mbaff
                && col_motion
                    .mb_field
                    .get(col_pos / 32)
                    .copied()
                    .unwrap_or(false),
        ),
    };
    let (ref0, poc, poc0, poc1) = if kind == ColKind::FieldFromMbaffField {
        // The co-located MB's field ref_idx names a field of frame entry
        // `col_ref_poc`: same parity as that MB when even, else the other
        let parity = if cur_bottom ^ (col_ref & 1 != 0) {
            PictureStructure::BottomField
        } else {
            PictureStructure::TopField
        };
        let ref0 = ref_pic_list_l0
            .iter()
            .position(|p| p.structure == parity && frame_poc_of(p, ref_pic_list_l0) == col_ref_poc)
            .unwrap_or(0);
        let poc0 = ref_pic_list_l0.get(ref0).map_or(0, |p| p.pic_order_cnt);
        (ref0, current_poc, poc0, col_pic.pic_order_cnt)
    } else if kind == ColKind::MbaffFieldFromField {
        // The referenced field, as a field ref_idx of this MB: frame entry
        // i, same parity first (2 * i) then the other (2 * i + 1)
        let (field_poc, ref_bottom) = (col_ref_poc >> 1, (col_ref_poc & 1) as usize);
        let parity = mb_idx % 2;
        let i = ref_pic_list_l0
            .iter()
            .position(|p| p.field_poc[ref_bottom] == field_poc)
            .unwrap_or(0);
        let k = 2 * i + (ref_bottom != parity) as usize;
        let poc0 = ref_pic_list_l0
            .get(i)
            .map_or(0, |p| p.field_poc[ref_bottom]);
        (
            k,
            current_field_poc[parity],
            poc0,
            col_pic.field_poc[parity],
        )
    } else if kind == ColKind::FrameFromField {
        // The frame (or field pair) holding the referenced field
        let (field_poc, bottom) = (col_ref_poc >> 1, (col_ref_poc & 1) as usize);
        let ref0 = ref_pic_list_l0
            .iter()
            .position(|p| p.field_poc[bottom] == field_poc)
            .unwrap_or(0);
        let poc0 = ref_pic_list_l0.get(ref0).map_or(0, |p| p.pic_order_cnt);
        (ref0, current_poc, poc0, col_pic.pic_order_cnt)
    } else if kind == ColKind::FieldFromFrame && !mctx.mbaff {
        // The field of the referenced frame with the current parity
        let frame_num = ref_pic_list_l0
            .iter()
            .find(|p| frame_poc_of(p, ref_pic_list_l0) == col_ref_poc)
            .map(|p| p.frame_num);
        let parity = if cur_bottom {
            PictureStructure::BottomField
        } else {
            PictureStructure::TopField
        };
        let ref0 = frame_num
            .and_then(|f| {
                ref_pic_list_l0
                    .iter()
                    .position(|p| p.frame_num == f && p.structure == parity)
            })
            .unwrap_or(0);
        let poc0 = ref_pic_list_l0.get(ref0).map_or(0, |p| p.pic_order_cnt);
        (ref0, current_poc, poc0, col_pic.pic_order_cnt)
    } else if cur_field {
        let parity = mb_idx % 2;
        // A field reference keeps its parity relative to the MB; a frame
        // reference becomes the same-parity field
        let k = 2 * frame_idx + if col_field { (col_ref & 1) as usize } else { 0 };
        let poc0 = ref_pic_list_l0
            .get(k / 2)
            .map_or(0, |p| p.field_poc[parity ^ (k & 1)]);
        (
            k,
            current_field_poc[parity],
            poc0,
            col_pic.field_poc[parity],
        )
    } else {
        let poc0 = ref_pic_list_l0
            .get(frame_idx)
            .map_or(0, |p| p.pic_order_cnt);
        (frame_idx, current_poc, poc0, col_pic.pic_order_cnt)
    };

    let td = (poc1 - poc0).clamp(-128, 127);
    // A long-term reference is not scaled: mvL0 = mvCol, mvL1 = 0
    let list_idx = if mctx.mbaff && cur_field {
        ref0 / 2
    } else {
        ref0
    };
    let long_term = long_term_l0.get(list_idx).copied().unwrap_or(false);
    let scale = if td == 0 || long_term {
        256
    } else {
        let tb = (poc - poc0).clamp(-128, 127);
        let tx = (16384 + (td.abs() >> 1)) / td;
        ((tb * tx + 32) >> 6).clamp(-1024, 1023)
    };
    let mvx = col_mv[0] as i32;
    let mvy = match (cur_field, col_field) {
        (true, false) => col_mv[1] as i32 / 2,
        (false, true) => col_mv[1] as i32 * 2,
        _ => col_mv[1] as i32,
    };
    let mx = (scale * mvx + 128) >> 8;
    let my = (scale * mvy + 128) >> 8;
    (
        [mx as i16, my as i16],
        [(mx - mvx) as i16, (my - mvy) as i16],
        ref0 as i8,
        0,
        true,
        true,
    )
}

/// Identity of a reference field as a field picture's co-located motion
/// records it: POC and parity.
pub(crate) fn field_ref_key(p: &DecodedPicture) -> i32 {
    p.pic_order_cnt
        .wrapping_mul(2)
        .wrapping_add((p.structure == PictureStructure::BottomField) as i32)
}

/// PicOrderCnt of the frame a field reference belongs to: its frame's for a
/// field view, else the smaller POC of it and its pair in `list`.
fn frame_poc_of(p: &DecodedPicture, list: &[Arc<DecodedPicture>]) -> i32 {
    if let Some(ViewSource::FieldOf(frame)) = &p.view_source {
        return frame.pic_order_cnt;
    }
    list.iter()
        .filter(|q| q.frame_num == p.frame_num && q.structure != p.structure)
        .map(|q| q.pic_order_cnt)
        .fold(p.pic_order_cnt, i32::min)
}

/// Motion vector prediction using the median of neighbors A, B, C (spec 8.4.1.3).
/// `part_idx`: partition index (0 for first/only partition).
/// `part_w`, `part_h`: partition dimensions.
/// `ref_idx`: reference index for this partition.
#[allow(clippy::too_many_arguments)]
pub(crate) fn predict_mv(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mb_idx: usize,
    mb_width: usize,
    part_idx: usize,
    part_w: usize,
    part_h: usize,
    ref_idx: i8,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mctx: MbaffCtx,
) -> (i16, i16) {
    let py_off = if part_h == 8 && part_w == 16 {
        part_idx * 8
    } else {
        0
    };
    let px_off = if part_w == 8 && part_h == 16 {
        part_idx * 8
    } else {
        0
    };

    // A: left neighbor (4x4 block to the left of partition's top-left)
    let a = get_mv_neighbor_left_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        py_off,
        px_off,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    );

    // B: above neighbor
    let b = get_mv_neighbor_above_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        py_off,
        px_off,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    );

    // C: above-right neighbor (or D: above-left if C unavailable)
    let c = get_mv_neighbor_above_right_mbaff(
        mv_store_l0,
        ref_idx_store_l0,
        mb_idx,
        mb_width,
        py_off,
        px_off,
        part_w,
        mb_slice_id,
        cur_slice_id,
        mctx.mbaff,
        mctx.mb_field_decoding,
    )
    .or_else(|| {
        get_mv_neighbor_above_left_mbaff(
            mv_store_l0,
            ref_idx_store_l0,
            mb_idx,
            mb_width,
            py_off,
            px_off,
            mb_slice_id,
            cur_slice_id,
            mctx.mbaff,
            mctx.mb_field_decoding,
        )
    });

    // Special cases for 16x8 and 8x16 (spec 8.4.1.3.1)
    if part_w == 16 && part_h == 8 {
        if part_idx == 0 {
            if let Some((mv, ri)) = b {
                if ri == ref_idx {
                    return (mv[0], mv[1]);
                }
            }
        } else if let Some((mv, ri)) = a {
            if ri == ref_idx {
                return (mv[0], mv[1]);
            }
        }
    }
    if part_w == 8 && part_h == 16 {
        if part_idx == 0 {
            if let Some((mv, ri)) = a {
                if ri == ref_idx {
                    return (mv[0], mv[1]);
                }
            }
        } else if let Some((mv, ri)) = c {
            if ri == ref_idx {
                return (mv[0], mv[1]);
            }
        }
    }

    // Count how many neighbors match the target ref_idx (spec 8.4.1.3.1)
    let ref_a = a.map(|(_, r)| r).unwrap_or(-1);
    let ref_b = b.map(|(_, r)| r).unwrap_or(-1);
    let ref_c = c.map(|(_, r)| r).unwrap_or(-1);
    let match_count =
        (ref_a == ref_idx) as u8 + (ref_b == ref_idx) as u8 + (ref_c == ref_idx) as u8;

    // When exactly one neighbor matches, use that neighbor's MV directly
    if match_count == 1 {
        if ref_a == ref_idx {
            if let Some((mv, _)) = a {
                return (mv[0], mv[1]);
            }
        }
        if ref_b == ref_idx {
            if let Some((mv, _)) = b {
                return (mv[0], mv[1]);
            }
        }
        if ref_c == ref_idx {
            if let Some((mv, _)) = c {
                return (mv[0], mv[1]);
            }
        }
    }

    // Special case (match_count == 0): when B and C are unavailable but A is,
    // use A's MV directly regardless of ref_idx (spec 8.4.1.3.1, H.264 clause).
    if let (None, None, Some((mv, _))) = (b, c, a) {
        return (mv[0], mv[1]);
    }

    // Otherwise: median predictor
    let mv_a = a.map(|(mv, _)| mv).unwrap_or([0, 0]);
    let mv_b = b.map(|(mv, _)| mv).unwrap_or([0, 0]);
    let mv_c = c.map(|(mv, _)| mv).unwrap_or([0, 0]);

    let mut xs = [mv_a[0], mv_b[0], mv_c[0]];
    let mut ys = [mv_a[1], mv_b[1], mv_c[1]];
    xs.sort();
    ys.sort();
    (xs[1], ys[1])
}

// ── MBAFF neighbor address helpers (spec 6.4.10-6.4.12) ──────────────────
//
// In MBAFF, MBs are indexed as CurrMbAddr = pair_addr * 2 + {0=top, 1=bottom}.
// Neighbor derivation depends on whether the current and neighbor MB pairs are
// frame-coded or field-coded.

/// MV and reference index of the 4x4 block covering luma location
/// `(xn, yn)` (relative to macroblock `mb_idx`, outside it) in an MBAFF
/// frame, or `None` if unavailable. Neighbour addressing follows spec
/// Table 6-4 (`mbaff_neighbor_loc`); a neighbour coded the other way is
/// scaled per spec 8.4.1.3.1: for a field macroblock next to a frame
/// macroblock the vertical MV is halved and the reference index doubled,
/// and the reverse for a frame macroblock next to a field macroblock.
#[allow(clippy::too_many_arguments)]
#[inline]
fn mbaff_mv_at(
    mv_store: &[[i16; 2]],
    ref_idx_store: &[i8],
    mb_idx: usize,
    mb_width: usize,
    xn: i32,
    yn: i32,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mb_field_decoding: &[bool],
) -> Option<([i16; 2], i8)> {
    let (mb_n, xw, yw) =
        crate::neighbor::mbaff_neighbor_loc(mb_idx, mb_width, mb_field_decoding, xn, yn, 16, 16)?;
    if mb_slice_id[mb_n] != cur_slice_id {
        return None;
    }
    let blk = OFFSET_TO_BLOCK[yw / 4][xw / 4];
    let mut mv = mv_store[mb_n * 16 + blk];
    let mut ref_idx = ref_idx_store[mb_n * 16 + blk];
    let cur_field = mb_field_decoding[mb_idx / 2];
    let n_field = mb_field_decoding[mb_n / 2];
    if cur_field && !n_field {
        mv[1] /= 2;
        if ref_idx >= 0 {
            ref_idx = ref_idx.saturating_mul(2);
        }
    } else if !cur_field && n_field {
        mv[1] = mv[1].saturating_mul(2);
        if ref_idx >= 0 {
            ref_idx >>= 1;
        }
    }
    Some((mv, ref_idx))
}

/// Get MV/ref of the left neighbor for a partition.
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_mv_neighbor_left_mbaff(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mb_idx: usize,
    mb_width: usize,
    py_off: usize,
    px_off: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> Option<([i16; 2], i8)> {
    if px_off > 0 {
        // Left is within this MB — unchanged for MBAFF
        let lr = py_off / 4;
        let lc = (px_off - 4) / 4;
        let blk = OFFSET_TO_BLOCK[lr][lc];
        Some((
            mv_store_l0[mb_idx * 16 + blk],
            ref_idx_store_l0[mb_idx * 16 + blk],
        ))
    } else if !mbaff {
        // Non-MBAFF: left is mb_idx - 1
        let mb_col = mb_idx % mb_width;
        if mb_col == 0 {
            return None;
        }
        let left_mb = mb_idx - 1;
        if mb_slice_id[left_mb] != cur_slice_id {
            return None;
        }
        let blk = OFFSET_TO_BLOCK[py_off / 4][3];
        Some((
            mv_store_l0[left_mb * 16 + blk],
            ref_idx_store_l0[left_mb * 16 + blk],
        ))
    } else {
        mbaff_mv_at(
            mv_store_l0,
            ref_idx_store_l0,
            mb_idx,
            mb_width,
            -1,
            py_off as i32,
            mb_slice_id,
            cur_slice_id,
            mb_field_decoding,
        )
    }
}

/// Get MV/ref of the above neighbor for a partition (MBAFF-aware).
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_mv_neighbor_above_mbaff(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mb_idx: usize,
    mb_width: usize,
    py_off: usize,
    px_off: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> Option<([i16; 2], i8)> {
    if py_off > 0 {
        // Above is within this MB — unchanged for MBAFF
        let lr = (py_off - 4) / 4;
        let lc = px_off / 4;
        let blk = OFFSET_TO_BLOCK[lr][lc];
        Some((
            mv_store_l0[mb_idx * 16 + blk],
            ref_idx_store_l0[mb_idx * 16 + blk],
        ))
    } else if !mbaff {
        // Non-MBAFF: above is mb_idx - mb_width
        let mb_row = mb_idx / mb_width;
        if mb_row == 0 {
            return None;
        }
        let above_mb = mb_idx - mb_width;
        if mb_slice_id[above_mb] != cur_slice_id {
            return None;
        }
        let blk = OFFSET_TO_BLOCK[3][px_off / 4];
        Some((
            mv_store_l0[above_mb * 16 + blk],
            ref_idx_store_l0[above_mb * 16 + blk],
        ))
    } else {
        mbaff_mv_at(
            mv_store_l0,
            ref_idx_store_l0,
            mb_idx,
            mb_width,
            px_off as i32,
            -1,
            mb_slice_id,
            cur_slice_id,
            mb_field_decoding,
        )
    }
}

/// Get MV/ref of the above-right neighbor for a partition (MBAFF-aware).
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_mv_neighbor_above_right_mbaff(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mb_idx: usize,
    mb_width: usize,
    py_off: usize,
    px_off: usize,
    part_w: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> Option<([i16; 2], i8)> {
    let right_col = px_off + part_w;

    if py_off > 0 {
        // Above-right within this MB — unchanged for MBAFF
        if right_col < 16 {
            // Per spec 6.4.11.7: the above-right 4x4 block is not available
            // if it would be in a different 8x8 block that hasn't been decoded.
            // Within P_8x8 / B_8x8, each 8x8 sub-MB is decoded independently.
            // The above-right at (py_off-4, px_off+part_w) is unavailable if
            // it crosses an 8x8 column boundary while staying in the same 8x8 row.
            let cur_8x8_col = px_off / 8;
            let tgt_col = right_col;
            let tgt_8x8_col = tgt_col / 8;
            // Also check row: above-right row is py_off-4
            let tgt_8x8_row = (py_off - 4) / 8;
            let cur_8x8_row = py_off / 8;
            let cur_8x8 = cur_8x8_row * 2 + cur_8x8_col;
            let tgt_8x8 = tgt_8x8_row * 2 + tgt_8x8_col;
            // Target must be in an already-decoded 8x8 block (lower index in scan)
            if tgt_8x8 > cur_8x8 {
                return None;
            }
            let lr = (py_off - 4) / 4;
            let lc = right_col / 4;
            let blk = OFFSET_TO_BLOCK[lr][lc];
            Some((
                mv_store_l0[mb_idx * 16 + blk],
                ref_idx_store_l0[mb_idx * 16 + blk],
            ))
        } else {
            None // right edge of MB, above-right is in MB above-right
        }
    } else if !mbaff && mb_idx / mb_width > 0 {
        // Non-MBAFF: above-right in MB above or above-right MB
        if right_col < 16 {
            let above_mb = mb_idx - mb_width;
            if mb_slice_id[above_mb] != cur_slice_id {
                return None;
            }
            let blk = OFFSET_TO_BLOCK[3][right_col / 4];
            Some((
                mv_store_l0[above_mb * 16 + blk],
                ref_idx_store_l0[above_mb * 16 + blk],
            ))
        } else if mb_idx % mb_width + 1 < mb_width {
            let above_right_mb = mb_idx - mb_width + 1;
            if mb_slice_id[above_right_mb] != cur_slice_id {
                return None;
            }
            let blk = OFFSET_TO_BLOCK[3][0];
            Some((
                mv_store_l0[above_right_mb * 16 + blk],
                ref_idx_store_l0[above_right_mb * 16 + blk],
            ))
        } else {
            None
        }
    } else if mbaff {
        // Above-right is the location just right of the partition, one row up
        // (in the above MB, or the above-right pair for the right column).
        mbaff_mv_at(
            mv_store_l0,
            ref_idx_store_l0,
            mb_idx,
            mb_width,
            right_col as i32,
            -1,
            mb_slice_id,
            cur_slice_id,
            mb_field_decoding,
        )
    } else {
        None
    }
}

/// Get MV/ref of the above-left neighbor for a partition (MBAFF-aware).
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_mv_neighbor_above_left_mbaff(
    mv_store_l0: &[[i16; 2]],
    ref_idx_store_l0: &[i8],
    mb_idx: usize,
    mb_width: usize,
    py_off: usize,
    px_off: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> Option<([i16; 2], i8)> {
    if py_off > 0 && px_off > 0 {
        // Within current MB — unchanged for MBAFF
        let blk = OFFSET_TO_BLOCK[(py_off - 4) / 4][(px_off - 4) / 4];
        return Some((
            mv_store_l0[mb_idx * 16 + blk],
            ref_idx_store_l0[mb_idx * 16 + blk],
        ));
    }

    if mbaff {
        return mbaff_mv_at(
            mv_store_l0,
            ref_idx_store_l0,
            mb_idx,
            mb_width,
            px_off as i32 - 1,
            py_off as i32 - 1,
            mb_slice_id,
            cur_slice_id,
            mb_field_decoding,
        );
    }

    // Cross-MB cases (not MBAFF)
    let (mb_col, mb_row) = (mb_idx % mb_width, mb_idx / mb_width);
    let (n_mb, n_blk) = if py_off == 0 && px_off == 0 {
        // Bottom-right block of the above-left MB
        if mb_row == 0 || mb_col == 0 {
            return None;
        }
        (mb_idx - mb_width - 1, OFFSET_TO_BLOCK[3][3])
    } else if py_off == 0 {
        // Above MB, column to the left of px_off
        if mb_row == 0 {
            return None;
        }
        (mb_idx - mb_width, OFFSET_TO_BLOCK[3][(px_off - 4) / 4])
    } else if px_off == 0 {
        // Left MB, row above py_off
        if mb_col == 0 {
            return None;
        }
        (mb_idx - 1, OFFSET_TO_BLOCK[(py_off - 4) / 4][3])
    } else {
        return None;
    };
    if mb_slice_id[n_mb] != cur_slice_id {
        return None;
    }
    Some((
        mv_store_l0[n_mb * 16 + n_blk],
        ref_idx_store_l0[n_mb * 16 + n_blk],
    ))
}
