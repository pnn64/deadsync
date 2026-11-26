//! Per-slice mutable decode state shared across all MB decode paths.
//!
//! `SliceContext` bundles the ~25 mutable arrays and geometry values that
//! every MB decoder (CABAC/CAVLC, I/P/B) reads and writes. Extracting it
//! from `decode_slice` enables splitting MB decode logic into methods.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::decoder::Frame;

#[allow(dead_code)] // Fields progressively used as methods are migrated
/// Read-only per-slice parameters used by MB decode methods.
///
/// Bundles the immutable slice-level data (header fields, PPS/SPS flags,
/// reference lists) to avoid long parameter lists.
pub(crate) struct SliceParams<'a> {
    pub is_p_slice: bool,
    pub is_b_slice: bool,
    pub use_weight: u8,
    pub current_poc: i32,
    /// TopFieldOrderCnt and BottomFieldOrderCnt of the current picture
    pub current_field_poc: [i32; 2],
    pub direct_spatial_mv_pred_flag: bool,
    pub direct_8x8_inference_flag: bool,
    pub transform_8x8_mode_flag: bool,
    pub scaling_list_4x4: &'a [[u8; 16]; 6],
    pub scaling_list_8x8: &'a [[u8; 64]; 2],
    pub constrained_intra_pred_flag: bool,
    pub chroma_qp_index_offset: i32,
    /// Chroma QP offset of Cr (spec 7.4.2.2)
    pub second_chroma_qp_index_offset: i32,
    /// Monochrome stream (chroma_format_idc 0): no chroma syntax; chroma
    /// planes stay mid-grey (intra chroma prediction is DC, CBP has no
    /// chroma bits)
    pub monochrome: bool,
    pub ref_pic_list: &'a [Arc<DecodedPicture>],
    pub ref_pic_list_l0: &'a [Arc<DecodedPicture>],
    pub ref_pic_list_l1: &'a [Arc<DecodedPicture>],
    /// Which entries of `ref_pic_list_l0` / `ref_pic_list_l1` are long-term
    pub long_term_l0: &'a [bool],
    pub long_term_l1: &'a [bool],
    pub num_ref_idx_l0_active: u32,
    pub num_ref_idx_l1_active: u32,
    pub wctx: &'a WeightContext<'a>,
    pub first_mb_in_slice: u32,
    pub slice_qp: i32,
    pub is_i_slice: bool,
    pub cabac_init_idc: u32,
}
use crate::dpb::{DecodedPicture, PictureStructure};
use crate::intra_pred::{
    predict_chroma, predict_intra_16x16, predict_intra_4x4, predict_intra_8x8,
};
use crate::mv_pred::{
    apply_spatial_direct_col_zero, derive_spatial_direct_base, derive_spatial_direct_blk,
    derive_temporal_direct_blk, predict_mv_skip, ref_pic_safe, MbaffCtx, WeightContext,
};
use crate::pixel::Pixel;
use crate::residual::BLOCK_INDEX_TO_OFFSET;

/// Mutable per-slice state passed to MB decode routines.
///
/// All mutable MB-level arrays live here so that individual decode
/// functions can be extracted as methods on `&mut SliceContext`.
#[allow(dead_code)] // Fields progressively used as methods are extracted from decode_slice
pub(crate) struct SliceContext<'a, P: Pixel = u8> {
    /// Sample type: planes hold `P::BYTES` bytes per sample
    pub _pixel: core::marker::PhantomData<P>,
    // Pixel output
    pub frame: &'a mut Frame,
    pub stride: usize,
    pub width: u32,
    pub height: u32,

    // Geometry
    pub mb_width: u32,

    // MBAFF state
    pub mbaff: bool,
    pub mb_field_decoding: &'a mut [bool],

    // Per-4x4-block coefficient counts (for CABAC CBF / CAVLC nC)
    pub nc_luma: &'a mut [u8],
    pub nc_cb: &'a mut [u8],
    pub nc_cr: &'a mut [u8],

    // Per-4x4-block motion vectors and reference indices
    pub mv_store_l0: &'a mut [[i16; 2]],
    pub mv_store_l1: &'a mut [[i16; 2]],
    pub ref_idx_store_l0: &'a mut [i8],
    pub ref_poc_store_l0: &'a mut [i32],
    pub ref_poc_store_l1: &'a mut [i32],
    pub ref_idx_store_l1: &'a mut [i8],

    // Per-4x4-block MVD (for CABAC amvd context)
    pub mvd_store: &'a mut [[i16; 2]],
    pub mvd_store_l1: &'a mut [[i16; 2]],

    // Per-MB deblocking / neighbor info
    pub mb_info: &'a mut [crate::deblock::MbInfo],

    // Per-4x4-block intra prediction modes
    pub i4x4_modes: &'a mut [u8],

    // Per-MB CABAC neighbor context state
    pub mb_cbp: &'a mut [u16],
    pub mb_chroma_pred: &'a mut [u8],
    pub mb_is_8x8dct: &'a mut [bool],
    pub mb_skip: &'a mut [bool],
    /// MBAFF CABAC: skip flag of the bottom MB, decoded ahead while
    /// handling a skipped top MB (its field flag, if any, applies to the
    /// top MB too); consumed by the bottom MB.
    pub mbaff_skip_lookahead: &'a mut Option<bool>,
    pub mb_is_direct: &'a mut [bool],
    pub blk_is_direct: &'a mut [bool],
    pub is_i16x16: &'a mut [bool],

    // Multi-slice boundary tracking
    pub mb_slice_id: &'a mut [u16],
    pub this_slice_id: u16,

    // QP state (carried across MBs within a slice)
    pub prev_mb_qp: i32,
    pub last_qp_delta_nonzero: bool,

    // Per-MB pixel layout (set by `set_mb_layout` at the start of each MB decode).
    // For frame-coded MBs: ly_stride = width, ly_offset = mb_y * width.
    // For field-coded MBs: ly_stride = width * 2, ly_offset = field_y_base.
    pub ly_stride: usize,
    pub ly_offset: usize,
    pub lc_stride: usize,
    pub lc_offset: usize,

    /// True if this is a field picture (field_pic_flag=1). Used to select
    /// field coefficient scan order (spec Table 8-13).
    pub field_pic_flag: bool,
    /// True if this is the bottom field (only meaningful when field_pic_flag=true).
    /// Used for chroma MV offset when referencing opposite-parity fields.
    pub bottom_field_flag: bool,
}

/// Standalone chroma field MV offset computation (avoids borrow issues).
pub(crate) fn chroma_field_mv_offset_impl(
    field_pic_flag: bool,
    bottom_field_flag: bool,
    ref_pic: &DecodedPicture,
) -> i32 {
    if !field_pic_flag {
        return 0;
    }
    let ref_is_bottom = ref_pic.structure == PictureStructure::BottomField;
    if bottom_field_flag == ref_is_bottom {
        0 // same parity
    } else if bottom_field_flag {
        2 // bottom referencing top: +2
    } else {
        -2 // top referencing bottom: -2
    }
}

/// [`SliceContext::chroma_mv_offset`] over explicit fields, for call sites
/// that hold a mutable borrow of the frame: `(field_pic_flag,
/// bottom_field_flag)` and `(mbaff, width, lc_stride, lc_offset)`.
pub(crate) fn chroma_mv_offset_impl(
    (field_pic_flag, bottom_field_flag): (bool, bool),
    (mbaff, width, lc_stride, lc_offset): (bool, u32, usize, usize),
    ref_pic: &DecodedPicture,
    c_ref_off: usize,
) -> i32 {
    if field_pic_flag {
        return chroma_field_mv_offset_impl(true, bottom_field_flag, ref_pic);
    }
    let cw = (width / 2) as usize;
    if mbaff && cw > 0 && lc_stride == 2 * cw {
        let cur_bottom = (lc_offset / cw) % 2 == 1;
        let ref_bottom = c_ref_off != 0;
        return 2 * (cur_bottom as i32 - ref_bottom as i32);
    }
    0
}

use crate::deblock::MbType;

impl<P: Pixel> SliceContext<'_, P> {
    /// I_PCM samples (spec 7.3.5): 256 luma, then the Cb and Cr samples
    /// of the MB, each from `next` (BitDepth bits as coded); monochrome
    /// chroma is mid-grey and not coded. Fails if `next` runs out.
    pub(crate) fn write_pcm_samples(
        &mut self,
        mut next: impl FnMut() -> Option<i32>,
        mb_x: usize,
        monochrome: bool,
    ) -> Result<(), crate::error::DecodeError> {
        for r in 0..16 {
            for c in 0..16 {
                let v = next().ok_or(crate::error::DecodeError::UnexpectedEof)?;
                let idx = self.ly_offset + r * self.ly_stride + mb_x + c;
                if (idx + 1) * P::BYTES <= self.frame.y.len() {
                    P::store(&mut self.frame.y, idx, P::clip(v));
                }
            }
        }
        let (cx, ch) = (mb_x / 2, 16 >> self.chroma_shift_y());
        for plane in 0..2 {
            for r in 0..ch {
                for c in 0..8 {
                    let v = if monochrome {
                        P::MID
                    } else {
                        next().ok_or(crate::error::DecodeError::UnexpectedEof)?
                    };
                    let idx = self.lc_offset + r * self.lc_stride + cx + c;
                    let dst = if plane == 0 {
                        &mut self.frame.u
                    } else {
                        &mut self.frame.v
                    };
                    if (idx + 1) * P::BYTES <= dst.len() {
                        P::store(dst, idx, P::clip(v));
                    }
                }
            }
        }
        Ok(())
    }

    /// QP_Y of the current MB from the previous MB's and mb_qp_delta
    /// (spec 7.4.5): wraps within -QpBdOffsetY..=51.
    #[inline]
    pub(crate) fn next_qp(&self, delta: i32) -> i32 {
        let o = P::QP_BD_OFFSET;
        (self.prev_mb_qp + delta + 52 + 2 * o).rem_euclid(52 + o) - o
    }

    /// Read a CAVLC mb_qp_delta, which spec 7.4.5 limits to
    /// [-(26 + QpBdOffsetY / 2), 25 + QpBdOffsetY / 2] (CABAC's unary code
    /// is capped instead, see `decode_mb_qp_delta`).
    pub(crate) fn read_mb_qp_delta(
        reader: &mut crate::bitstream::BitstreamReader,
    ) -> Result<i32, crate::error::DecodeError> {
        let half = P::QP_BD_OFFSET / 2;
        match reader.read_se()? {
            d if (-(26 + half)..=25 + half).contains(&d) => Ok(d),
            _ => Err(crate::error::DecodeError::InvalidSyntax(
                "mb_qp_delta out of range",
            )),
        }
    }

    /// log2 of the luma rows per chroma row: 1 for 4:2:0, 0 for 4:2:2.
    #[inline]
    pub(crate) fn chroma_shift_y(&self) -> usize {
        self.frame.chroma_format.shift_y() as usize
    }

    /// Whether the chroma planes are 4:2:2 (8x16 chroma per MB).
    #[inline]
    pub(crate) fn chroma_422(&self) -> bool {
        self.frame.chroma_format == crate::decoder::ChromaFormat::Yuv422
    }

    /// Returns true if the current MB uses field coefficient scan order.
    /// True for field pictures and for field-coded MBs in MBAFF.
    pub(crate) fn is_field_scan(&self, mb_idx: usize) -> bool {
        self.field_pic_flag || (self.mbaff && self.mb_field_decoding[mb_idx / 2])
    }

    /// Chroma MV vertical offset (in 1/8 chroma samples) for the current MB
    /// predicting from `ref_pic`, whose chroma data starts at `c_ref_off` (from
    /// `mc_params`). Field pictures and MBAFF field MBs referencing the field
    /// of opposite parity get +2 (bottom from top) or -2 (top from bottom)
    /// (spec 8.4.1.4). In an MBAFF field MB the current parity is the MB's
    /// position in its pair and the reference parity is the field `mc_params`
    /// chose, i.e. `c_ref_off != 0` for the bottom field.
    pub(crate) fn chroma_mv_offset(&self, ref_pic: &DecodedPicture, c_ref_off: usize) -> i32 {
        chroma_mv_offset_impl(
            (self.field_pic_flag, self.bottom_field_flag),
            (self.mbaff, self.width, self.lc_stride, self.lc_offset),
            ref_pic,
            c_ref_off,
        )
    }

    /// Set per-MB pixel layout (stride and y-offset) based on field/frame coding.
    /// Must be called at the start of each MB decode.
    pub(crate) fn set_mb_layout(&mut self, mb_idx: usize, mb_x: usize, mb_y: usize) {
        let w = self.stride;
        let cw = (self.width / 2) as usize;
        let sy = self.chroma_shift_y();
        if self.mbaff && self.mb_field_decoding[mb_idx / 2] {
            // Field-coded MB pair: doubled stride, field-line offset
            let pair_row = (mb_idx / 2) / self.mb_width as usize;
            let pair_y = pair_row * 32;
            let is_bottom = !mb_idx.is_multiple_of(2);
            self.ly_stride = w * 2;
            self.ly_offset = (pair_y + if is_bottom { 1 } else { 0 }) * w;
            self.lc_stride = cw * 2;
            let pair_cy = (pair_row * 32) >> sy; // chroma pair y
            self.lc_offset = (pair_cy + if is_bottom { 1 } else { 0 }) * cw;
        } else {
            // Frame-coded (progressive or MBAFF frame-coded pair)
            self.ly_stride = w;
            self.ly_offset = mb_y * w;
            self.lc_stride = cw;
            self.lc_offset = (mb_y >> sy) * cw;
        }
        let _ = mb_x; // mb_x not needed for layout, only mb_y
    }

    /// Returns MC parameters for the current field-coded or frame-coded MB.
    /// `ref_idx` is the decoded reference index for the partition being MC'd.
    /// Returns `(mc_y, ref_stride, ref_y_offset, mc_cy, c_ref_stride, c_ref_offset)`.
    ///
    /// For field-coded MBs, the reference index is in field units:
    /// - Even ref_idx → same-parity field (top→top, bottom→bottom)
    /// - Odd ref_idx → opposite-parity field (top→bottom, bottom→top)
    ///
    /// The actual frame reference is at `ref_list[ref_idx / 2]`.
    #[inline]
    pub(crate) fn mc_params(
        &self,
        mb_idx: usize,
        mb_y: usize,
        ref_width: usize,
        ref_idx: i8,
    ) -> (i32, usize, usize, usize, usize, usize) {
        if self.mbaff && self.mb_field_decoding[mb_idx / 2] {
            let pair_row = (mb_idx / 2) / self.mb_width as usize;
            let is_bottom = !mb_idx.is_multiple_of(2);
            let mc_y = (pair_row * 16) as i32;
            let ref_stride = ref_width * 2;
            // Determine which field of the reference to read:
            // Even ref_idx → same parity, odd ref_idx → opposite parity
            let ref_is_bottom = is_bottom ^ (ref_idx % 2 != 0);
            let ref_y_off = if ref_is_bottom { ref_width } else { 0 };
            let mc_cy = (pair_row * 16) >> self.chroma_shift_y();
            let cw = ref_width / 2;
            let c_ref_stride = cw * 2;
            let c_ref_off = if ref_is_bottom { cw } else { 0 };
            (mc_y, ref_stride, ref_y_off, mc_cy, c_ref_stride, c_ref_off)
        } else {
            (
                mb_y as i32,
                ref_width,
                0,
                mb_y >> self.chroma_shift_y(),
                ref_width / 2,
                0,
            )
        }
    }

    /// Map a field-coded ref_idx to the frame-level reference list index.
    /// For field-coded MBs, ref_idx is doubled (2 fields per frame ref).
    /// For frame-coded, returns ref_idx unchanged.
    #[inline]
    pub(crate) fn frame_ref_idx(&self, mb_idx: usize, ref_idx: i8) -> i8 {
        if self.mbaff && self.mb_field_decoding[mb_idx / 2] {
            ref_idx / 2
        } else {
            ref_idx
        }
    }

    /// Returns true if the current MB is field-coded (for CABAC context selection).
    /// True for field pictures and for field-coded MBs in MBAFF.
    #[inline]
    pub(crate) fn is_field_coded(&self, mb_idx: usize) -> bool {
        self.field_pic_flag || (self.mbaff && self.mb_field_decoding[mb_idx / 2])
    }

    /// Field-coded MBs double the count (each frame ref → 2 field refs).
    #[inline]
    pub(crate) fn effective_num_ref(&self, mb_idx: usize, num_ref: u32) -> u32 {
        if self.mbaff && self.mb_field_decoding[mb_idx / 2] {
            (num_ref * 2).min(32)
        } else {
            num_ref
        }
    }

    /// Get the left MB index for context lookups.
    /// Returns `None` if no left MB exists or it's in a different slice.
    #[inline]
    pub(crate) fn left_mb(&self, mb_idx: usize) -> Option<usize> {
        if !self.mbaff {
            let mb_col = mb_idx % self.mb_width as usize;
            if mb_col == 0 {
                return None;
            }
            let left = mb_idx - 1;
            if self.mb_slice_id[left] != self.this_slice_id {
                return None;
            }
            Some(left)
        } else {
            let pair_addr = mb_idx / 2;
            let pair_col = pair_addr % self.mb_width as usize;
            if pair_col == 0 {
                return None;
            }
            let left_pair = pair_addr - 1;
            let cur_field = self.mb_field_decoding[pair_addr];
            let left_field = self.mb_field_decoding[left_pair];
            // Per-MB contexts (skip, mb_type, cbp, ...) use the MB covering
            // luma location (-1, 0) (spec 6.4.11.1 via Table 6-4). That is the
            // same-position MB of the left pair when both pairs are coded the
            // same way, but the left pair's top MB for a bottom MB when they
            // differ: line 0 of a frame bottom MB is frame line 16 (the top
            // field MB's line 8), and line 0 of a field bottom MB is frame
            // line 1 (in the top frame MB).
            let is_top = mb_idx.is_multiple_of(2);
            let left_mb = if is_top || cur_field != left_field {
                left_pair * 2
            } else {
                left_pair * 2 + 1
            };
            if self.mb_slice_id[left_mb] != self.this_slice_id {
                return None;
            }
            Some(left_mb)
        }
    }

    /// Get the above MB index for context lookups.
    /// Returns `None` if no above MB exists or it's in a different slice.
    #[inline]
    pub(crate) fn above_mb(&self, mb_idx: usize) -> Option<usize> {
        if !self.mbaff {
            if mb_idx < self.mb_width as usize {
                return None;
            }
            let above = mb_idx - self.mb_width as usize;
            if self.mb_slice_id[above] != self.this_slice_id {
                return None;
            }
            Some(above)
        } else {
            let is_top = mb_idx.is_multiple_of(2);
            let pair_addr = mb_idx / 2;
            let is_field = self.mb_field_decoding[pair_addr];
            if !is_top && !is_field {
                // Frame-coded bottom MB: above is top MB of same pair
                let above = mb_idx - 1;
                if self.mb_slice_id[above] != self.this_slice_id {
                    return None;
                }
                Some(above)
            } else if !is_top && is_field {
                // Field-coded bottom MB: above is bottom of above pair (same field)
                let pair_row = pair_addr / self.mb_width as usize;
                if pair_row == 0 {
                    return None;
                }
                let above_pair = pair_addr - self.mb_width as usize;
                let above_mb = above_pair * 2 + 1; // bottom of above pair (same field)
                if self.mb_slice_id[above_mb] != self.this_slice_id {
                    return None;
                }
                Some(above_mb)
            } else {
                // Top MB (spec Table 6-4, yN < 0): the bottom MB of the above
                // pair, except for a field top MB below a field pair, whose
                // above neighbour is the same-parity (top) field MB.
                let pair_row = pair_addr / self.mb_width as usize;
                if pair_row == 0 {
                    return None;
                }
                let above_pair = pair_addr - self.mb_width as usize;
                let above_mb = if is_field && self.mb_field_decoding[above_pair] {
                    above_pair * 2
                } else {
                    above_pair * 2 + 1
                };
                if self.mb_slice_id[above_mb] != self.this_slice_id {
                    return None;
                }
                Some(above_mb)
            }
        }
    }

    /// Check if a neighbor MB is available for intra prediction samples.
    /// When `constrained_intra_pred_flag` is set in a P/B slice, inter-predicted
    /// neighbors are treated as unavailable (spec 6.4.1).
    pub(crate) fn is_intra_neighbor_avail(&self, neighbor_idx: usize, sp: &SliceParams) -> bool {
        if !sp.constrained_intra_pred_flag || !(sp.is_p_slice || sp.is_b_slice) {
            return true; // no constraint, neighbor is available
        }
        let mt = self.mb_info[neighbor_idx].mb_type;
        mt == MbType::Intra || mt == MbType::Ipcm
    }

    /// Availability of the left column for intra prediction, for luma rows
    /// 0-7 and 8-15 (chroma rows 0-3 and 4-7). In an MBAFF frame the left
    /// samples of the two halves can come from different MBs of the left
    /// pair (spec Table 6-4), which matters with constrained intra
    /// prediction, where an inter MB's samples are unavailable.
    pub(crate) fn left_intra_avail(&self, mb_idx: usize, sp: &SliceParams) -> [bool; 2] {
        if !self.mbaff {
            let l = self
                .left_mb(mb_idx)
                .is_some_and(|left| self.is_intra_neighbor_avail(left, sp));
            return [l, l];
        }
        let mbw = self.mb_width as usize;
        [0, 8].map(|y0| {
            (y0..y0 + 8).all(|y| {
                crate::neighbor::mbaff_neighbor_loc(
                    mb_idx,
                    mbw,
                    self.mb_field_decoding,
                    -1,
                    y,
                    16,
                    16,
                )
                .is_some_and(|(m, _, _)| {
                    self.mb_slice_id[m] == self.this_slice_id && self.is_intra_neighbor_avail(m, sp)
                })
            })
        })
    }

    /// Whether the macroblock holding the luma sample at `(xn, yn)` relative
    /// to `mb_idx` (above-left `(-1, -1)` or above-right `(16, -1)`) is
    /// available for intra prediction: decoded, in this slice, and intra when
    /// `constrained_intra_pred_flag` applies. MBAFF uses spec Table 6-4, so a
    /// bottom field MB has an above-right neighbour while a bottom frame MB
    /// does not.
    pub(crate) fn intra_corner_avail(
        &self,
        mb_idx: usize,
        xn: i32,
        yn: i32,
        sp: &SliceParams,
    ) -> bool {
        let mbw = self.mb_width as usize;
        let n = if self.mbaff {
            crate::neighbor::mbaff_neighbor_loc(mb_idx, mbw, self.mb_field_decoding, xn, yn, 16, 16)
                .map(|(m, _, _)| m)
        } else {
            let col = (mb_idx % mbw) as i32
                + if xn < 0 {
                    -1
                } else if xn >= 16 {
                    1
                } else {
                    0
                };
            (mb_idx >= mbw && (0..mbw as i32).contains(&col))
                .then(|| mb_idx - mbw - (mb_idx % mbw) + col as usize)
        };
        n.is_some_and(|m| {
            self.mb_slice_id[m] == self.this_slice_id && self.is_intra_neighbor_avail(m, sp)
        })
    }

    /// Build per-MB intra availability array for predict_i4x4_mode.
    /// When constrained_intra_pred_flag is off or in I-slices, all MBs are available.
    pub(crate) fn intra_avail_map(&self, sp: &SliceParams) -> Vec<bool> {
        if !sp.constrained_intra_pred_flag || !(sp.is_p_slice || sp.is_b_slice) {
            vec![true; self.mb_info.len()]
        } else {
            self.mb_info
                .iter()
                .map(|info| info.mb_type == MbType::Intra || info.mb_type == MbType::Ipcm)
                .collect()
        }
    }

    /// Decode a P-slice skip macroblock: median MV prediction, MC, no residual.
    pub(crate) fn decode_p_skip_mb(
        &mut self,
        mb_idx: usize,
        mb_x: usize,
        mb_y: usize,
        sp: &SliceParams,
    ) {
        let ref_pic_list = sp.ref_pic_list;
        let wctx = sp
            .wctx
            .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx);
        let use_weight = sp.use_weight;
        let (mvp_x, mvp_y) = predict_mv_skip(
            self.mv_store_l0,
            self.ref_idx_store_l0,
            mb_idx,
            self.mb_width as usize,
            self.mb_slice_id,
            self.this_slice_id,
            MbaffCtx {
                mbaff: self.mbaff,
                mb_field_decoding: self.mb_field_decoding,
            },
        );
        if let Some(ref_pic) = ref_pic_list.first() {
            let (mc_y, ref_stride, ref_y_off, _, _, _) =
                self.mc_params(mb_idx, mb_y, ref_pic.width as usize, 0i8);
            // Luma MC
            let mut luma_pred = [P::default(); 256];
            P::luma_mc(
                ref_pic,
                mb_x as i32,
                mc_y,
                mvp_x as i32,
                mvp_y as i32,
                16,
                16,
                &mut luma_pred,
                ref_stride,
                ref_y_off,
            );
            if use_weight == 1 {
                wctx.apply_uni(&mut luma_pred, 0, 0, false, 0);
            }
            for (r, row) in luma_pred.as_chunks::<16>().0.iter().enumerate() {
                let o = self.ly_offset + r * self.ly_stride + mb_x;
                P::store_row(&mut self.frame.y, o, row);
            }
            self.predict_chroma_part(
                mb_idx,
                mb_x,
                mb_y,
                sp,
                (0, 0, 16, 16),
                Some((ref_pic, 0, [mvp_x, mvp_y])),
                None,
            );
        }
        // Store MVs and ref indices
        for blk in 0..16 {
            self.mv_store_l0[mb_idx * 16 + blk] = [mvp_x, mvp_y];
            self.ref_idx_store_l0[mb_idx * 16 + blk] = 0;
        }
    }

    /// Derive direct-mode MVs for all 16 4x4 blocks of a macroblock.
    ///
    /// Used by B_Skip, B_Direct_16x16, and B_Direct_8x8 sub-partitions.
    pub(crate) fn derive_direct_mvs(
        &mut self,
        mb_idx: usize,
        blk_start: usize,
        blk_count: usize,
        sp: &SliceParams,
    ) {
        let direct_spatial = sp.direct_spatial_mv_pred_flag;
        // Spatial direct zeroes MVs from a still co-located block only when
        // RefPicList1[0] is a short-term reference (spec 8.4.1.2.2)
        let col_pic_spatial = sp
            .ref_pic_list_l1
            .first()
            .filter(|_| !sp.long_term_l1.first().copied().unwrap_or(false))
            .map(|p| p.as_ref());
        let direct_8x8_inference_flag = sp.direct_8x8_inference_flag;
        let current_poc = sp.current_poc;
        let ref_pic_list_l0 = sp.ref_pic_list_l0;
        let ref_pic_list_l1 = sp.ref_pic_list_l1;
        // When direct_8x8_inference_flag is set, all 4 blocks within each 8x8
        // group get the same result: the neighbor-derived MV/ref uses MB-level
        // position (0,0), and the co-located lookup maps to the same representative
        // block per 8x8 group. Derive once per group and fill all 4 sub-blocks.
        if direct_spatial {
            if direct_8x8_inference_flag && blk_count == 4 {
                // Single 8x8 group: derive once, fill 4 blocks
                let first_blk = blk_start;
                let (mv_l0, mv_l1, ri_l0, ri_l1, _, _) = derive_spatial_direct_blk(
                    self.mv_store_l0,
                    self.ref_idx_store_l0,
                    self.mv_store_l1,
                    self.ref_idx_store_l1,
                    mb_idx,
                    self.mb_width as usize,
                    col_pic_spatial,
                    first_blk,
                    self.mb_slice_id,
                    self.this_slice_id,
                    direct_8x8_inference_flag,
                    MbaffCtx {
                        mbaff: self.mbaff,
                        mb_field_decoding: self.mb_field_decoding,
                    },
                    current_poc,
                    self.bottom_field_flag,
                );
                let base = mb_idx * 16;
                for blk in blk_start..blk_start + 4 {
                    self.mv_store_l0[base + blk] = mv_l0;
                    self.ref_idx_store_l0[base + blk] = ri_l0;
                    self.mv_store_l1[base + blk] = mv_l1;
                    self.ref_idx_store_l1[base + blk] = ri_l1;
                }
            } else if direct_8x8_inference_flag && blk_count == 16 {
                // Full MB: the spatial neighbor-derived result is MB-invariant;
                // only the co-located zero-MV refinement varies by 8x8 representative.
                let base = mb_idx * 16;
                let spatial_base = derive_spatial_direct_base(
                    [
                        (self.mv_store_l0, self.ref_idx_store_l0),
                        (self.mv_store_l1, self.ref_idx_store_l1),
                    ],
                    mb_idx,
                    self.mb_width as usize,
                    self.mb_slice_id,
                    self.this_slice_id,
                    MbaffCtx {
                        mbaff: self.mbaff,
                        mb_field_decoding: self.mb_field_decoding,
                    },
                );
                let col_pic = col_pic_spatial;
                for group in 0..4 {
                    let first_blk = group * 4;
                    let (mv_l0, mv_l1, ri_l0, ri_l1, _, _) = apply_spatial_direct_col_zero(
                        spatial_base,
                        col_pic,
                        mb_idx,
                        first_blk,
                        direct_8x8_inference_flag,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                        current_poc,
                        self.bottom_field_flag,
                    );
                    for blk in first_blk..first_blk + 4 {
                        self.mv_store_l0[base + blk] = mv_l0;
                        self.ref_idx_store_l0[base + blk] = ri_l0;
                        self.mv_store_l1[base + blk] = mv_l1;
                        self.ref_idx_store_l1[base + blk] = ri_l1;
                    }
                }
            } else {
                // No inference flag: per-block derivation
                for blk in blk_start..blk_start + blk_count {
                    let (mv_l0, mv_l1, ri_l0, ri_l1, _, _) = derive_spatial_direct_blk(
                        self.mv_store_l0,
                        self.ref_idx_store_l0,
                        self.mv_store_l1,
                        self.ref_idx_store_l1,
                        mb_idx,
                        self.mb_width as usize,
                        col_pic_spatial,
                        blk,
                        self.mb_slice_id,
                        self.this_slice_id,
                        direct_8x8_inference_flag,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                        current_poc,
                        self.bottom_field_flag,
                    );
                    self.mv_store_l0[mb_idx * 16 + blk] = mv_l0;
                    self.ref_idx_store_l0[mb_idx * 16 + blk] = ri_l0;
                    self.mv_store_l1[mb_idx * 16 + blk] = mv_l1;
                    self.ref_idx_store_l1[mb_idx * 16 + blk] = ri_l1;
                }
            }
        } else {
            let Some(col_pic) = ref_pic_list_l1.first() else {
                return; // malformed: temporal direct requires L1 ref list
            };
            if direct_8x8_inference_flag && blk_count == 4 {
                let first_blk = blk_start;
                let (mv_l0, mv_l1, ri_l0, ri_l1, _, _) = derive_temporal_direct_blk(
                    col_pic,
                    ref_pic_list_l0,
                    sp.long_term_l0,
                    (current_poc, sp.current_field_poc),
                    mb_idx,
                    first_blk,
                    direct_8x8_inference_flag,
                    MbaffCtx {
                        mbaff: self.mbaff,
                        mb_field_decoding: self.mb_field_decoding,
                    },
                    self.bottom_field_flag,
                );
                let base = mb_idx * 16;
                for blk in blk_start..blk_start + 4 {
                    self.mv_store_l0[base + blk] = mv_l0;
                    self.ref_idx_store_l0[base + blk] = ri_l0;
                    self.mv_store_l1[base + blk] = mv_l1;
                    self.ref_idx_store_l1[base + blk] = ri_l1;
                }
            } else if direct_8x8_inference_flag && blk_count == 16 {
                let base = mb_idx * 16;
                for group in 0..4 {
                    let first_blk = group * 4;
                    let (mv_l0, mv_l1, ri_l0, ri_l1, _, _) = derive_temporal_direct_blk(
                        col_pic,
                        ref_pic_list_l0,
                        sp.long_term_l0,
                        (current_poc, sp.current_field_poc),
                        mb_idx,
                        first_blk,
                        direct_8x8_inference_flag,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                        self.bottom_field_flag,
                    );
                    for blk in first_blk..first_blk + 4 {
                        self.mv_store_l0[base + blk] = mv_l0;
                        self.ref_idx_store_l0[base + blk] = ri_l0;
                        self.mv_store_l1[base + blk] = mv_l1;
                        self.ref_idx_store_l1[base + blk] = ri_l1;
                    }
                }
            } else {
                for blk in blk_start..blk_start + blk_count {
                    let (mv_l0, mv_l1, ri_l0, ri_l1, _, _) = derive_temporal_direct_blk(
                        col_pic,
                        ref_pic_list_l0,
                        sp.long_term_l0,
                        (current_poc, sp.current_field_poc),
                        mb_idx,
                        blk,
                        direct_8x8_inference_flag,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                        self.bottom_field_flag,
                    );
                    self.mv_store_l0[mb_idx * 16 + blk] = mv_l0;
                    self.ref_idx_store_l0[mb_idx * 16 + blk] = ri_l0;
                    self.mv_store_l1[mb_idx * 16 + blk] = mv_l1;
                    self.ref_idx_store_l1[mb_idx * 16 + blk] = ri_l1;
                }
            }
        }
    }

    /// Fast path for B-skip macroblocks whose direct-derived MV/ref state is uniform.
    ///
    /// This is bit-identical to the per-4x4 path because luma/chroma MC and
    /// weighted prediction are pure per-pixel functions of absolute block
    /// coordinates once the MV/ref tuple is identical for every 4x4 block.
    fn try_decode_uniform_b_skip_mb(
        &mut self,
        mb_idx: usize,
        mb_x: usize,
        mb_y: usize,
        sp: &SliceParams,
    ) -> bool {
        let base = mb_idx * 16;
        let mv0 = self.mv_store_l0[base];
        let mv1 = self.mv_store_l1[base];
        let r0 = self.ref_idx_store_l0[base];
        let r1 = self.ref_idx_store_l1[base];
        let uniform = (1..16).all(|blk| {
            self.mv_store_l0[base + blk] == mv0
                && self.mv_store_l1[base + blk] == mv1
                && self.ref_idx_store_l0[base + blk] == r0
                && self.ref_idx_store_l1[base + blk] == r1
        });
        if !uniform {
            return false;
        }

        let bp0 = r0 >= 0;
        let bp1 = r1 >= 0;
        let ref_l0 = if bp0 {
            match ref_pic_safe(sp.ref_pic_list_l0, self.frame_ref_idx(mb_idx, r0)) {
                Some(pic) => Some(pic),
                None => return true,
            }
        } else {
            None
        };
        let ref_l1 = if bp1 {
            match ref_pic_safe(sp.ref_pic_list_l1, self.frame_ref_idx(mb_idx, r1)) {
                Some(pic) => Some(pic),
                None => return true,
            }
        } else {
            None
        };

        let mut luma_pred = [P::default(); 256];
        match (ref_l0, ref_l1) {
            (Some(ref0), Some(ref1)) => {
                let mut p0 = [P::default(); 256];
                let mut p1 = [P::default(); 256];
                let (mc_y_l0, ref_stride_l0, ref_y_off_l0, _, _, _) =
                    self.mc_params(mb_idx, mb_y, ref0.width as usize, r0);
                let (mc_y_l1, ref_stride_l1, ref_y_off_l1, _, _, _) =
                    self.mc_params(mb_idx, mb_y, ref1.width as usize, r1);
                P::luma_mc(
                    ref0,
                    mb_x as i32,
                    mc_y_l0,
                    mv0[0] as i32,
                    mv0[1] as i32,
                    16,
                    16,
                    &mut p0,
                    ref_stride_l0,
                    ref_y_off_l0,
                );
                P::luma_mc(
                    ref1,
                    mb_x as i32,
                    mc_y_l1,
                    mv1[0] as i32,
                    mv1[1] as i32,
                    16,
                    16,
                    &mut p1,
                    ref_stride_l1,
                    ref_y_off_l1,
                );
                sp.wctx
                    .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx)
                    .apply_bi(&p0, &p1, &mut luma_pred, r0 as usize, r1 as usize, false, 0);
            }
            (Some(ref0), None) => {
                let (mc_y, ref_stride, ref_y_off, _, _, _) =
                    self.mc_params(mb_idx, mb_y, ref0.width as usize, r0);
                P::luma_mc(
                    ref0,
                    mb_x as i32,
                    mc_y,
                    mv0[0] as i32,
                    mv0[1] as i32,
                    16,
                    16,
                    &mut luma_pred,
                    ref_stride,
                    ref_y_off,
                );
                if sp.use_weight == 1 {
                    sp.wctx
                        .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx)
                        .apply_uni(&mut luma_pred, 0, r0 as usize, false, 0);
                }
            }
            (None, Some(ref1)) => {
                let (mc_y, ref_stride, ref_y_off, _, _, _) =
                    self.mc_params(mb_idx, mb_y, ref1.width as usize, r1);
                P::luma_mc(
                    ref1,
                    mb_x as i32,
                    mc_y,
                    mv1[0] as i32,
                    mv1[1] as i32,
                    16,
                    16,
                    &mut luma_pred,
                    ref_stride,
                    ref_y_off,
                );
                if sp.use_weight == 1 {
                    sp.wctx
                        .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx)
                        .apply_uni(&mut luma_pred, 1, r1 as usize, false, 0);
                }
            }
            (None, None) => {}
        }
        for (r, row) in luma_pred.as_chunks::<16>().0.iter().enumerate() {
            let o = self.ly_offset + r * self.ly_stride + mb_x;
            P::store_row(&mut self.frame.y, o, row);
        }

        self.predict_chroma_part(
            mb_idx,
            mb_x,
            mb_y,
            sp,
            (0, 0, 16, 16),
            ref_l0.map(|pic| (&**pic, r0, mv0)),
            ref_l1.map(|pic| (&**pic, r1, mv1)),
        );

        true
    }

    /// Decode a B-slice skip macroblock: spatial/temporal direct MV derivation,
    /// per-4x4-block MC (luma + chroma), no residual.
    /// Decode a B-slice skip macroblock: spatial/temporal direct MV derivation,
    /// per-4x4-block MC (luma + chroma), no residual.
    pub(crate) fn decode_b_skip_mb(
        &mut self,
        mb_idx: usize,
        mb_x: usize,
        mb_y: usize,
        sp: &SliceParams,
    ) {
        let ref_pic_list_l0 = sp.ref_pic_list_l0;
        let ref_pic_list_l1 = sp.ref_pic_list_l1;
        let wctx = sp
            .wctx
            .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx);
        let use_weight = sp.use_weight;
        // Derive MVs per 4x4 block via spatial or temporal direct mode
        self.derive_direct_mvs(mb_idx, 0, 16, sp);

        if self.try_decode_uniform_b_skip_mb(mb_idx, mb_x, mb_y, sp) {
            self.mb_is_direct[mb_idx] = true;
            for blk in 0..16 {
                self.blk_is_direct[mb_idx * 16 + blk] = true;
            }
            return;
        }

        // Prediction partitions: one per 8x8 whose four 4x4 blocks share
        // motion (always, with direct_8x8_inference), else one per 4x4
        // (direct without it can give each 4x4 its own motion).
        // (luma block, luma rectangle)
        let mut cparts = [(0usize, (0usize, 0usize, 0usize, 0usize)); 16];
        let mut n_cparts = 0;
        for b8 in 0..4 {
            let base = mb_idx * 16 + b8 * 4;
            let same = (1..4).all(|k| {
                self.mv_store_l0[base + k] == self.mv_store_l0[base]
                    && self.mv_store_l1[base + k] == self.mv_store_l1[base]
                    && self.ref_idx_store_l0[base + k] == self.ref_idx_store_l0[base]
                    && self.ref_idx_store_l1[base + k] == self.ref_idx_store_l1[base]
            });
            if same {
                cparts[n_cparts] = (b8 * 4, ((b8 % 2) * 8, (b8 / 2) * 8, 8, 8));
                n_cparts += 1;
            } else {
                for (blk, &(row, col)) in BLOCK_INDEX_TO_OFFSET
                    .iter()
                    .enumerate()
                    .skip(b8 * 4)
                    .take(4)
                {
                    cparts[n_cparts] = (blk, (col, row, 4, 4));
                    n_cparts += 1;
                }
            }
        }
        // Luma MC per partition, straight into the frame
        for &(blk, (px, py, pw, ph)) in &cparts[..n_cparts] {
            let i = mb_idx * 16 + blk;
            let (mv0, mv1) = (self.mv_store_l0[i], self.mv_store_l1[i]);
            let (r0, r1) = (self.ref_idx_store_l0[i], self.ref_idx_store_l1[i]);
            let n = pw * ph;
            let mut pred = [P::default(); 64];
            let mc =
                |ctx: &Self, list: &[Arc<DecodedPicture>], r: i8, mv: [i16; 2], out: &mut [P]| {
                    let Some(pic) = ref_pic_safe(list, ctx.frame_ref_idx(mb_idx, r)) else {
                        return false;
                    };
                    let (mc_y, ref_stride, ref_y_off, _, _, _) =
                        ctx.mc_params(mb_idx, mb_y, pic.width as usize, r);
                    P::luma_mc(
                        pic,
                        (mb_x + px) as i32,
                        mc_y + py as i32,
                        mv[0] as i32,
                        mv[1] as i32,
                        pw,
                        ph,
                        out,
                        ref_stride,
                        ref_y_off,
                    );
                    true
                };
            if r0 >= 0 && r1 >= 0 {
                let mut p0 = [P::default(); 64];
                let mut p1 = [P::default(); 64];
                if !mc(self, ref_pic_list_l0, r0, mv0, &mut p0[..n])
                    || !mc(self, ref_pic_list_l1, r1, mv1, &mut p1[..n])
                {
                    return;
                }
                wctx.apply_bi(
                    &p0[..n],
                    &p1[..n],
                    &mut pred[..n],
                    r0 as usize,
                    r1 as usize,
                    false,
                    0,
                );
            } else if r0 >= 0 {
                if !mc(self, ref_pic_list_l0, r0, mv0, &mut pred[..n]) {
                    return;
                }
                if use_weight == 1 {
                    wctx.apply_uni(&mut pred[..n], 0, r0 as usize, false, 0);
                }
            } else if r1 >= 0 {
                if !mc(self, ref_pic_list_l1, r1, mv1, &mut pred[..n]) {
                    return;
                }
                if use_weight == 1 {
                    wctx.apply_uni(&mut pred[..n], 1, r1 as usize, false, 0);
                }
            }
            for r in 0..ph {
                let o = self.ly_offset + (py + r) * self.ly_stride + mb_x + px;
                P::store_row(&mut self.frame.y, o, &pred[r * pw..(r + 1) * pw]);
            }
        }

        for &(luma_blk, rect) in &cparts[..n_cparts] {
            let i = mb_idx * 16 + luma_blk;
            let (r0, r1) = (self.ref_idx_store_l0[i], self.ref_idx_store_l1[i]);
            let mut m0 = None;
            let mut m1 = None;
            if r0 >= 0 {
                let Some(pic) = ref_pic_safe(ref_pic_list_l0, self.frame_ref_idx(mb_idx, r0))
                else {
                    return;
                };
                m0 = Some((&**pic, r0, self.mv_store_l0[i]));
            }
            if r1 >= 0 {
                let Some(pic) = ref_pic_safe(ref_pic_list_l1, self.frame_ref_idx(mb_idx, r1))
                else {
                    return;
                };
                m1 = Some((&**pic, r1, self.mv_store_l1[i]));
            }
            self.predict_chroma_part(mb_idx, mb_x, mb_y, sp, rect, m0, m1);
        }
        self.mb_is_direct[mb_idx] = true;
        for blk in 0..16 {
            self.blk_is_direct[mb_idx * 16 + blk] = true;
        }
    }

    /// Gather CABAC CBP neighbor context for a macroblock.
    ///
    /// Returns `(left_cbp, top_cbp, left_cbp_chroma, top_cbp_chroma)` for use
    /// with `decode_cbp_luma` and `decode_cbp_chroma`.
    pub(crate) fn cabac_cbp_context(&self, mb_idx: usize, is_intra: bool) -> (u8, u8, u8, u8) {
        let unavail_cbp: u16 = if is_intra { 0x7CF } else { 0x00F };
        let mut left_cbp_raw = if let Some(left) = self.left_mb(mb_idx) {
            self.mb_cbp[left]
        } else {
            unavail_cbp
        };
        if self.mbaff {
            // The left 8x8 luma block of each 8x8 row comes from the sample
            // left of that row (spec Table 6-4), which in a mixed frame/field
            // pair may be the other MB or the other 8x8 row of the left pair.
            // Bits 1 and 3 carry the left blocks of rows 0 and 1.
            let mbw = self.mb_width as usize;
            for (row, bit) in [(0usize, 1u16), (1, 3)] {
                let cbp_bit = crate::neighbor::mbaff_neighbor_loc(
                    mb_idx,
                    mbw,
                    self.mb_field_decoding,
                    -1,
                    8 * row as i32,
                    16,
                    16,
                )
                .filter(|&(m, _, _)| self.mb_slice_id[m] == self.this_slice_id)
                .map_or((unavail_cbp >> bit) & 1, |(m, _, yw)| {
                    (self.mb_cbp[m] >> (1 + 2 * (yw / 8) as u16)) & 1
                });
                left_cbp_raw = (left_cbp_raw & !(1 << bit)) | (cbp_bit << bit);
            }
        }
        let top_cbp_raw = if let Some(above) = self.above_mb(mb_idx) {
            self.mb_cbp[above]
        } else {
            unavail_cbp
        };
        let left_cbp =
            ((left_cbp_raw & 0x7F0) | (left_cbp_raw & 2) | (((left_cbp_raw >> 2) & 2) << 2)) as u8;
        let top_cbp = top_cbp_raw as u8;
        let left_cbp_c = ((left_cbp_raw >> 4) & 3) as u8;
        let top_cbp_c = ((top_cbp_raw >> 4) & 3) as u8;
        (left_cbp, top_cbp, left_cbp_c, top_cbp_c)
    }

    /// Compute intra chroma prediction for both U and V planes.
    ///
    /// Gathers neighbor samples (respecting slice boundaries), calls `predict_chroma_8x8`
    /// for each plane, and returns the two 64-byte prediction buffers.
    /// Compute I16x16 luma prediction and add residual to the frame.
    ///
    /// Gathers neighbor samples (respecting slice boundaries), calls `predict_intra_16x16`,
    /// adds the 16x16 residual, clamps, and writes to `frame.y`.
    /// Predict and reconstruct a single I4x4 luma block.
    ///
    /// Gathers above/left/above-left neighbor samples (respecting slice boundaries
    /// and above-right availability per spec 6.4.12), calls `predict_intra_4x4`,
    /// adds residual, clamps, and writes to `frame.y`.
    #[allow(clippy::too_many_arguments)]
    /// Predict and reconstruct a single I8x8 luma block (8x8 transform).
    ///
    /// Similar to I4x4 but operates on 8x8 blocks with 16-sample above buffer
    /// and 8-sample left buffer.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reconstruct_luma_8x8_block(
        &mut self,
        i8x8: usize,
        mb_x: usize,
        pred_mode: u8,
        luma_residual: &[i32; 256],
        above_mb_avail: bool,
        left_mb_avail: [bool; 2],
        above_left_mb_avail: bool,
        above_right_mb_avail: bool,
    ) {
        let row_off = (i8x8 / 2) * 8;
        let col_off = (i8x8 % 2) * 8;
        let px = mb_x + col_off;

        // Above samples (16: 8 above + 8 above-right)
        let above_base = self.ly_offset + row_off * self.ly_stride;
        let above_avail = above_base >= self.ly_stride && (row_off > 0 || above_mb_avail);
        let above_buf: Option<[P; 16]> = if above_avail {
            let mut buf = [P::default(); 16];
            let above_row = above_base - self.ly_stride;
            for (i, b) in buf.iter_mut().enumerate().take(8) {
                *b = P::load(&self.frame.y, above_row + px + i);
            }
            let has_tr = if row_off == 0 {
                if px + 8 < (mb_x + 16).min(self.stride) {
                    true
                } else if px + 8 < self.stride {
                    above_right_mb_avail
                } else {
                    false
                }
            } else {
                col_off == 0
            };
            if has_tr {
                for i in 0..8 {
                    let col = (px + 8 + i).min(self.stride - 1);
                    buf[8 + i] = P::load(&self.frame.y, above_row + col);
                }
            } else {
                let last = buf[7];
                buf[8..].fill(last);
            }
            Some(buf)
        } else {
            None
        };

        // Left samples
        let left_avail = px > 0 && (col_off > 0 || left_mb_avail[row_off / 8]);
        let left_buf: Option<[P; 8]> = if left_avail {
            let mut buf = [P::default(); 8];
            for (i, b) in buf.iter_mut().enumerate() {
                *b = P::load(
                    &self.frame.y,
                    self.ly_offset + (row_off + i) * self.ly_stride + px - 1,
                );
            }
            Some(buf)
        } else {
            None
        };

        // Above-left
        let al_avail = px > 0
            && above_base >= self.ly_stride
            && ((row_off > 0 && col_off > 0)
                || (row_off > 0 && col_off == 0 && left_mb_avail[(row_off - 1) / 8])
                || (row_off == 0 && col_off > 0 && above_mb_avail)
                || (row_off == 0 && col_off == 0 && above_left_mb_avail));
        let above_left_val = if al_avail {
            Some(P::load(&self.frame.y, above_base - self.ly_stride + px - 1))
        } else {
            None
        };

        let has_topright = above_buf.is_some()
            && ((row_off == 0 && px + 8 < self.stride) || (row_off > 0 && col_off == 0));

        let mut pred = [P::default(); 64];
        predict_intra_8x8(
            pred_mode,
            above_buf.as_ref().map(|b| &b[..]),
            left_buf.as_ref().map(|b| &b[..]),
            above_left_val,
            has_topright,
            &mut pred,
        );
        crate::residual::put_pred_residual(
            &mut self.frame.y,
            self.ly_offset + row_off * self.ly_stride + px,
            self.ly_stride,
            &pred,
            8,
            &luma_residual[row_off * 16 + col_off..],
            16,
            (8, 8),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reconstruct_luma_4x4_block(
        &mut self,
        px: usize,
        py: usize,
        mb_x: usize,
        mb_y: usize,
        blk: usize,
        pred_mode: u8,
        block_coeffs: &[i32; 16],
        above_mb_avail: bool,
        left_mb_avail: [bool; 2],
        above_left_mb_avail: bool,
        above_right_mb_avail: bool,
    ) {
        let local_row = py - mb_y;
        let local_col = px - mb_x;

        // Above samples
        let above_base = self.ly_offset + local_row * self.ly_stride;
        let above_avail = above_base >= self.ly_stride && (local_row > 0 || above_mb_avail);
        let above_buf: Option<[P; 8]> = if above_avail {
            let mut buf = [P::default(); 8];
            let above_row = above_base - self.ly_stride;
            for (i, b) in buf.iter_mut().enumerate().take(4) {
                *b = P::load(&self.frame.y, above_row + px + i);
            }
            let topright_avail = if local_row == 0 {
                if px + 4 < (mb_x + 16).min(self.stride) {
                    true
                } else if px + 4 < self.stride {
                    above_right_mb_avail
                } else {
                    false
                }
            } else {
                !matches!(blk, 3 | 7 | 11 | 13 | 15)
            };
            if topright_avail {
                for (i, b) in buf.iter_mut().enumerate().skip(4) {
                    let col = (px + i).min(self.stride - 1);
                    *b = P::load(&self.frame.y, above_row + col);
                }
            } else {
                let last = buf[3];
                buf[4..8].fill(last);
            }
            Some(buf)
        } else {
            None
        };

        // Left samples
        let left_avail = px > 0 && (local_col > 0 || left_mb_avail[local_row / 8]);
        let left_buf: Option<[P; 4]> = if left_avail {
            let mut buf = [P::default(); 4];
            for (i, b) in buf.iter_mut().enumerate() {
                *b = P::load(
                    &self.frame.y,
                    self.ly_offset + (local_row + i) * self.ly_stride + px - 1,
                );
            }
            Some(buf)
        } else {
            None
        };

        // Above-left sample
        let al_avail = px > 0
            && above_base >= self.ly_stride
            && ((local_row > 0 && local_col > 0)
                || (local_row > 0 && local_col == 0 && left_mb_avail[(local_row - 1) / 8])
                || (local_row == 0 && local_col > 0 && above_mb_avail)
                || (local_row == 0 && local_col == 0 && above_left_mb_avail));
        let above_left_val = if al_avail {
            Some(P::load(&self.frame.y, above_base - self.ly_stride + px - 1))
        } else {
            None
        };

        // Predict + add residual + write
        let mut pred = [P::default(); 16];
        predict_intra_4x4(
            pred_mode,
            above_buf.as_ref().map(|b| &b[..]),
            left_buf.as_ref().map(|b| &b[..]),
            above_left_val,
            &mut pred,
        );
        crate::residual::put_pred_residual(
            &mut self.frame.y,
            self.ly_offset + local_row * self.ly_stride + px,
            self.ly_stride,
            &pred,
            4,
            block_coeffs,
            4,
            (4, 4),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reconstruct_luma_16x16(
        &mut self,
        mb_x: usize,
        pred_mode: u8,
        luma_residual: &[i32; 256],
        above_avail: bool,
        left_avail: bool,
        above_left_avail: bool,
    ) {
        let above: Option<[P; 16]> = (self.ly_offset >= self.ly_stride && above_avail).then(|| {
            core::array::from_fn(|x| {
                P::load(&self.frame.y, self.ly_offset - self.ly_stride + mb_x + x)
            })
        });
        let left: Option<[P; 16]> = (mb_x > 0 && left_avail).then(|| {
            core::array::from_fn(|y| {
                P::load(
                    &self.frame.y,
                    self.ly_offset + y * self.ly_stride + mb_x - 1,
                )
            })
        });
        let above_left = if mb_x > 0 && self.ly_offset >= self.ly_stride && above_left_avail {
            Some(P::load(
                &self.frame.y,
                self.ly_offset - self.ly_stride + mb_x - 1,
            ))
        } else {
            None
        };
        let mut luma_pred = [P::default(); 256];
        predict_intra_16x16(
            pred_mode,
            above.as_ref().map(|v| &v[..]),
            left.as_ref().map(|v| &v[..]),
            above_left,
            &mut luma_pred,
        );
        crate::residual::put_pred_residual(
            &mut self.frame.y,
            self.ly_offset + mb_x,
            self.ly_stride,
            &luma_pred,
            16,
            luma_residual,
            16,
            (16, 16),
        );
    }

    /// Compute intra chroma prediction for both U and V planes.
    /// Intra chroma prediction of the current MB written to both chroma
    /// planes of the frame.
    pub(crate) fn write_chroma_intra_pred(
        &mut self,
        mb_x: usize,
        intra_chroma_pred_mode: u8,
        above_avail: bool,
        left_avail: [bool; 2],
        above_left_avail: bool,
    ) {
        let cx = mb_x / 2;
        let h = 16 >> self.chroma_shift_y();
        let (base, stride) = (self.lc_offset, self.lc_stride);
        for plane in 0..2 {
            let buf: &mut [u8] = if plane == 0 {
                &mut self.frame.u
            } else {
                &mut self.frame.v
            };
            let above = (base >= stride && above_avail).then(|| {
                let mut a = [P::default(); 8];
                P::load_row(buf, base - stride + cx, &mut a);
                a
            });
            let left = (cx > 0 && (left_avail[0] || left_avail[1])).then(|| {
                let mut l = [P::default(); 16];
                for (r, v) in l.iter_mut().enumerate().take(h) {
                    *v = P::load(buf, base + r * stride + cx - 1);
                }
                l
            });
            let above_left = (cx > 0 && base >= stride && above_left_avail)
                .then(|| P::load(buf, base - stride + cx - 1));
            let mut pred = [P::default(); 128];
            predict_chroma(
                intra_chroma_pred_mode,
                above.as_ref().map(|a| &a[..]),
                left.as_ref().map(|l| &l[..h]),
                left_avail,
                above_left,
                h,
                &mut pred,
            );
            for r in 0..h {
                let o = base + r * stride + cx;
                P::store_row(buf, o, &pred[r * 8..r * 8 + 8]);
            }
        }
    }

    ///
    /// Called once after the MB loop completes, covering MBs `first_mb..last_mb`.
    /// Fill the deblocking info of MBs `first_mb..last_mb` (the per-MB part
    /// of finalizing a slice; [`fill_ref_poc_table`](Self::fill_ref_poc_table)
    /// is the rest). Each MB depends only on its
    /// own decoded data and the slice's reference lists, so this can run
    /// incrementally as MB rows complete.
    pub(crate) fn finalize_mb_info_range(
        &mut self,
        first_mb: usize,
        last_mb: usize,
        sp: &SliceParams,
    ) {
        let list_count = if sp.is_b_slice {
            2u8
        } else if sp.is_p_slice {
            1
        } else {
            0
        };
        let l0_list = if sp.is_p_slice {
            sp.ref_pic_list
        } else {
            sp.ref_pic_list_l0
        };
        let ref_pic_list_l1 = sp.ref_pic_list_l1;

        #[allow(clippy::needless_range_loop)]
        for mi in first_mb..last_mb {
            let info = &mut self.mb_info[mi];
            let base = mi * 16;
            info.list_count = list_count;
            info.is_8x8dct = self.mb_is_8x8dct[mi];
            // Identity of the referenced picture for the deblocking bS. An
            // MBAFF field MB references fields: ref_idx k is the field of
            // parity (MB parity) ^ (k & 1) of frame entry k / 2, told apart
            // by its field POC and parity.
            let field_parity = (self.mbaff && self.mb_field_decoding[mi / 2]).then_some(mi % 2);
            let ref_id = |list: &[Arc<DecodedPicture>], ri: i8| -> i32 {
                if ri < 0 {
                    return -1;
                }
                let ri = ri as usize;
                match field_parity {
                    Some(p) => list.get(ri / 2).map_or(-1, |pic| {
                        let parity = p ^ (ri & 1);
                        pic.field_poc[parity]
                            .wrapping_mul(2)
                            .wrapping_add(parity as i32)
                    }),
                    // Field pictures: both fields of a frame can share a POC
                    None if self.field_pic_flag => list
                        .get(ri)
                        .map_or(-1, |pic| crate::mv_pred::field_ref_key(pic)),
                    None => list.get(ri).map_or(-1, |pic| pic.pic_order_cnt),
                }
            };
            // Blocks mostly share a reference: look each ref_idx up once
            let mut last = [(i8::MIN, 0i32); 2];
            let mut cached_id = |l: usize, list: &[Arc<DecodedPicture>], ri: i8| {
                if last[l].0 != ri {
                    last[l] = (ri, ref_id(list, ri));
                }
                last[l].1
            };
            for blk in 0..16 {
                info.mv_l0[blk] = self.mv_store_l0[base + blk];
                info.ref_idx_l0[blk] = self.ref_idx_store_l0[base + blk];
                info.mv_l1[blk] = self.mv_store_l1[base + blk];
                info.ref_idx_l1[blk] = self.ref_idx_store_l1[base + blk];
                info.nnz[blk] = self.nc_luma[base + blk] > 0;
                info.ref_poc_l0[blk] = cached_id(0, l0_list, self.ref_idx_store_l0[base + blk]);
                info.ref_poc_l1[blk] =
                    cached_id(1, ref_pic_list_l1, self.ref_idx_store_l1[base + blk]);
            }
            fn uniform<T: PartialEq + Copy>(a: &[T; 16]) -> bool {
                a.iter().all(|&x| x == a[0])
            }
            info.uniform_motion = uniform(&info.mv_l0)
                && uniform(&info.ref_poc_l0)
                && uniform(&info.mv_l1)
                && uniform(&info.ref_poc_l1)
                && uniform(&info.ref_idx_l0)
                && uniform(&info.ref_idx_l1);
            // An 8x8 transform block has coefficients if any of its 4x4
            // blocks does (CAVLC codes it as four interleaved 4x4 parts).
            if info.is_8x8dct {
                for q in info.nnz.as_chunks_mut::<4>().0 {
                    let any = q.iter().any(|&n| n);
                    q.fill(any);
                }
            }
        }
    }

    /// Build the per-block reference POC table for temporal direct mode
    /// (spec 8.4.1.2.3) from this slice's L0 list.
    /// Fill the per-block reference frame POCs of MBs `from..to` (this
    /// slice). A field MB's ref_idx addresses fields, two per frame entry.
    pub(crate) fn fill_ref_poc_table(&mut self, from: usize, to: usize, sp: &SliceParams) {
        let l0_list = if sp.is_p_slice {
            sp.ref_pic_list
        } else {
            sp.ref_pic_list_l0
        };
        let to = to.min(self.ref_poc_store_l0.len() / 16);
        for mb in from..to {
            let shift = (self.mbaff && self.mb_field_decoding[mb / 2]) as usize;
            for i in mb * 16..mb * 16 + 16 {
                // A field picture's references are fields, told apart by
                // parity as well as POC (both fields may share a POC)
                let field_pic = self.field_pic_flag;
                let frame_poc = |list: &[Arc<DecodedPicture>], ri: i8| {
                    (ri >= 0)
                        .then(|| list.get(ri as usize >> shift))
                        .flatten()
                        .map_or(-1, |p| {
                            if field_pic {
                                crate::mv_pred::field_ref_key(p)
                            } else {
                                p.pic_order_cnt
                            }
                        })
                };
                self.ref_poc_store_l0[i] = frame_poc(l0_list, self.ref_idx_store_l0[i]);
                self.ref_poc_store_l1[i] = frame_poc(sp.ref_pic_list_l1, self.ref_idx_store_l1[i]);
            }
        }
    }
}
