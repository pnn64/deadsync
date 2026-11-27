//! Mode info decoding: `intra_frame_mode_info`/`inter_frame_mode_info` (spec §6.4.6, §6.4.11)
//! and everything they call directly -- segment id (spec §6.4.7/§6.4.9/§6.4.12/§6.4.14),
//! `skip`/`tx_size` (spec §6.4.8/§6.4.10), `is_inter`/`ref_frames` (spec §6.4.13/§6.4.17), and
//! motion vector syntax (spec §6.4.18-6.4.20). Reference-frame neighbor-context derivation
//! (`comp_mode_ctx` and friends) lives in `tile::ref_ctx`; motion vector *prediction*
//! (`find_mv_refs` and friends) lives in `tile::mv_pred`.

use super::mv_pred::{use_mv_hp, Mv, ZERO_MV};
use super::{MiInfo, TileDecoder, TileError};
use crate::bool_coder::BoolDecoder;
use crate::common::INTRA_FRAME;
use crate::header::{SEG_LVL_REF_FRAME, SEG_LVL_SKIP};
use crate::prob_tables::{
    ALTREF_FRAME, BLOCK_8X8, COMPOUND_REFERENCE, DC_PRED, GOLDEN_FRAME, INTERP_FILTER_TREE,
    INTER_MODE_TREE, INTRA_MODE_TREE, KF_UV_MODE_PROBS, KF_Y_MODE_PROBS, LAST_FRAME,
    MAX_TXSIZE_LOOKUP, MV_CLASS_TREE, MV_FR_TREE, MV_JOINT_HNZVNZ, MV_JOINT_HNZVZ, MV_JOINT_HZVNZ,
    MV_JOINT_TREE, NEARESTMV, NEARMV, NEWMV, NUM_4X4_BLOCKS_HIGH_LOOKUP,
    NUM_4X4_BLOCKS_WIDE_LOOKUP, NUM_8X8_BLOCKS_HIGH_LOOKUP, NUM_8X8_BLOCKS_WIDE_LOOKUP,
    REFERENCE_MODE_SELECT, REF_NONE, SEGMENT_TREE, SINGLE_REFERENCE, SIZE_GROUP_LOOKUP, SWITCHABLE,
    TX_16X16, TX_32X32, TX_MODE_SELECT, TX_MODE_TO_BIGGEST_TX_SIZE, TX_SIZE_16_TREE,
    TX_SIZE_32_TREE, TX_SIZE_8_TREE, ZEROMV,
};

/// Neighbor information derived by `inter_frame_mode_info( )` (spec §6.4.11)
/// (`LeftRefFrame`/`AboveRefFrame`/`LeftIntra`/`AboveIntra`/`LeftSingle`/`AboveSingle`).
/// `pub(super)` (rather than private): read by `tile::ref_ctx`'s context-derivation methods.
pub(super) struct NeighborRefInfo {
    pub(super) avail_u: bool,
    pub(super) avail_l: bool,
    pub(super) left_ref_frame: [u8; 2],
    pub(super) above_ref_frame: [u8; 2],
    pub(super) left_intra: bool,
    pub(super) above_intra: bool,
    pub(super) left_single: bool,
    pub(super) above_single: bool,
}

/// Sentinel used by [`TileDecoder::interp_filter_ctx`] when a neighbor is unavailable, intra,
/// or disagrees with the other neighbor (spec §9.3.2).
const INTERP_FILTER_UNSET: u8 = 3;

/// Fills `sub_modes` for a sub-8x8 intra block (`mi_size < BLOCK_8X8`) by calling `read_sub`
/// at each `(idy, idx)` sub-block origin; returns `(last_mode, sub_modes)`.
fn decode_sub8x8_intra_modes<F: FnMut(u32, u32, &[u8; 4]) -> u8>(
    mi_size: u8,
    mut read_sub: F,
) -> (u8, [u8; 4]) {
    let mut sub_modes = [DC_PRED; 4];
    let num4x4w = NUM_4X4_BLOCKS_WIDE_LOOKUP[mi_size as usize] as u32;
    let num4x4h = NUM_4X4_BLOCKS_HIGH_LOOKUP[mi_size as usize] as u32;
    let mut last_mode = DC_PRED;
    let mut idy = 0u32;
    while idy < 2 {
        let mut idx = 0u32;
        while idx < 2 {
            let mode = read_sub(idy, idx, &sub_modes);
            for y2 in 0..num4x4h {
                for x2 in 0..num4x4w {
                    sub_modes[((idy + y2) * 2 + idx + x2) as usize] = mode;
                }
            }
            last_mode = mode;
            idx += num4x4w;
        }
        idy += num4x4h;
    }
    (last_mode, sub_modes)
}

impl TileDecoder<'_> {
    /// `seg_feature_active( feature )` (spec §6.4.9).
    fn seg_feature_active(&self, segment_id: u8, feature: usize) -> bool {
        self.segmentation.enabled && self.segmentation.feature_enabled[segment_id as usize][feature]
    }

    /// `intra_segment_id( )` (spec §6.4.7). Used for `FrameIsIntra` blocks
    /// (key frames / intra-only frames), which have no temporal prediction of
    /// `segment_id` (unlike `inter_segment_id`).
    fn intra_segment_id(&self, r: &mut BoolDecoder) -> u8 {
        if self.segmentation.enabled && self.segmentation.update_map {
            r.read_tree(&SEGMENT_TREE, |node| self.segmentation.tree_probs[node]) as u8
        } else {
            0
        }
    }

    /// `get_segment_id( )` (spec §6.4.14). The predicted segment id is the
    /// smallest value found in the on-screen region of `PrevSegmentIds`
    /// covered by the current block.
    fn get_segment_id(&self, row: u32, col: u32, mi_size: u8) -> Result<u8, TileError> {
        let bw = NUM_8X8_BLOCKS_WIDE_LOOKUP[mi_size as usize] as u32;
        let bh = NUM_8X8_BLOCKS_HIGH_LOOKUP[mi_size as usize] as u32;
        let xmis = (self.mi_cols - col).min(bw);
        let ymis = (self.mi_rows - row).min(bh);
        let mut seg = 7u8;
        for y in 0..ymis {
            for x in 0..xmis {
                let idx = ((row + y) * self.mi_cols + (col + x)) as usize;
                let prev = *self
                    .prev_segment_ids
                    .get(idx)
                    .ok_or(TileError::CorruptTile)?;
                seg = seg.min(prev);
            }
        }
        Ok(seg)
    }

    /// `inter_segment_id( )` (spec §6.4.12). Used for blocks in an inter frame
    /// (`!FrameIsIntra`), whether or not the individual block itself is inter-coded.
    fn inter_segment_id(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        mi_size: u8,
    ) -> Result<u8, TileError> {
        if !self.segmentation.enabled {
            return Ok(0);
        }
        let predicted_segment_id = self.get_segment_id(row, col, mi_size)?;
        if !self.segmentation.update_map {
            return Ok(predicted_segment_id);
        }
        if !self.segmentation.temporal_update {
            return Ok(r.read_tree(&SEGMENT_TREE, |node| self.segmentation.tree_probs[node]) as u8);
        }

        let ctx = (self.left_seg_pred_context[(row % 8) as usize]
            + self.above_ctx.seg_pred[col as usize]) as usize;
        let seg_id_predicted = r.read_bool(self.segmentation.pred_prob[ctx]);
        let segment_id = if seg_id_predicted {
            predicted_segment_id
        } else {
            r.read_tree(&SEGMENT_TREE, |node| self.segmentation.tree_probs[node]) as u8
        };

        let bw = NUM_8X8_BLOCKS_WIDE_LOOKUP[mi_size as usize] as u32;
        let bh = NUM_8X8_BLOCKS_HIGH_LOOKUP[mi_size as usize] as u32;
        for i in 0..bw {
            self.above_ctx.seg_pred[(col + i) as usize] = seg_id_predicted as u8;
        }
        for i in 0..bh {
            self.left_seg_pred_context[((row + i) % 8) as usize] = seg_id_predicted as u8;
        }
        Ok(segment_id)
    }

    /// `read_skip( )` (spec §6.4.8).
    fn read_skip(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        avail_u: bool,
        avail_l: bool,
        segment_id: u8,
    ) -> bool {
        if self.seg_feature_active(segment_id, SEG_LVL_SKIP) {
            return true;
        }
        let mut ctx = 0usize;
        // SAFETY: `avail_u` guarantees `row > 0` and `(row - 1, col)` is in this tile/strip;
        // `avail_l` guarantees `col > self.mi_col_start` and `(row, col - 1)` is in this strip.
        if avail_u && unsafe { self.mi_grid.get_unchecked(row - 1, col) }.skip() {
            ctx += 1;
        }
        if avail_l && unsafe { self.mi_grid.get_unchecked(row, col - 1) }.skip() {
            ctx += 1;
        }
        let skip = r.read_bool(self.probs.skip_prob[ctx]);
        if !self.frame_is_intra {
            self.count(|c| c.skip[ctx][skip as usize] += 1);
        }
        skip
    }

    /// `read_tx_size( allowSelect )` (spec §6.4.10).
    fn read_tx_size(
        &mut self,
        r: &mut BoolDecoder,
        mi_size: u8,
        allow_select: bool,
        pos: (u32, u32),
        avail: (bool, bool),
    ) -> u8 {
        let (row, col) = pos;
        let (avail_u, avail_l) = avail;
        let max_tx_size = MAX_TXSIZE_LOOKUP[mi_size as usize];
        if allow_select && self.tx_mode == TX_MODE_SELECT && mi_size >= BLOCK_8X8 {
            let mut above = max_tx_size;
            let mut left = max_tx_size;
            if avail_u {
                // SAFETY: `avail_u` guarantees `row > 0` and `(row - 1, col)` is in strip bounds.
                let n = unsafe { self.mi_grid.get_unchecked(row - 1, col) };
                if !n.skip() {
                    above = n.tx_size();
                }
            }
            if avail_l {
                // SAFETY: `avail_l` guarantees `col > mi_col_start` and `(row, col - 1)` is in strip bounds.
                let n = unsafe { self.mi_grid.get_unchecked(row, col - 1) };
                if !n.skip() {
                    left = n.tx_size();
                }
            }
            if !avail_l {
                left = above;
            }
            if !avail_u {
                above = left;
            }
            let ctx = ((above as u32 + left as u32) > max_tx_size as u32) as usize;
            let probs = self.probs.tx_probs[max_tx_size as usize][ctx];
            let tx_size = match max_tx_size {
                TX_32X32 => r.read_tree(&TX_SIZE_32_TREE, |node| probs[node]) as u8,
                TX_16X16 => r.read_tree(&TX_SIZE_16_TREE, |node| probs[node]) as u8,
                _ => r.read_tree(&TX_SIZE_8_TREE, |node| probs[node]) as u8,
            };
            if !self.frame_is_intra {
                self.count(|c| c.tx_size[max_tx_size as usize][ctx][tx_size as usize] += 1);
            }
            tx_size
        } else {
            max_tx_size.min(TX_MODE_TO_BIGGEST_TX_SIZE[self.tx_mode as usize])
        }
    }

    /// `intra_frame_mode_info( )` (spec §6.4.6).
    pub(super) fn intra_frame_mode_info(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        mi_size: u8,
        avail_u: bool,
        avail_l: bool,
    ) -> Result<MiInfo, TileError> {
        let segment_id = self.intra_segment_id(r);
        let skip = self.read_skip(r, row, col, avail_u, avail_l, segment_id);
        let tx_size = self.read_tx_size(r, mi_size, true, (row, col), (avail_u, avail_l));

        let (y_mode, sub_modes) = if mi_size >= BLOCK_8X8 {
            let above_mode = if avail_u {
                // SAFETY: `avail_u` guarantees `(row - 1, col)` is in strip bounds.
                unsafe { self.mi_grid.get_unchecked(row - 1, col) }.sub_modes()[2]
            } else {
                DC_PRED
            };
            let left_mode = if avail_l {
                // SAFETY: `avail_l` guarantees `(row, col - 1)` is in strip bounds.
                unsafe { self.mi_grid.get_unchecked(row, col - 1) }.sub_modes()[1]
            } else {
                DC_PRED
            };
            let mode = r.read_tree(&INTRA_MODE_TREE, |node| {
                KF_Y_MODE_PROBS[above_mode as usize][left_mode as usize][node]
            }) as u8;
            (mode, [mode; 4])
        } else {
            decode_sub8x8_intra_modes(mi_size, |idy, idx, sub_modes| {
                let above_mode = if idy > 0 {
                    sub_modes[idx as usize]
                } else if avail_u {
                    // SAFETY: `avail_u` guarantees `(row - 1, col)` is in strip bounds.
                    unsafe { self.mi_grid.get_unchecked(row - 1, col) }.sub_modes()
                        [(2 + idx) as usize]
                } else {
                    DC_PRED
                };
                let left_mode = if idx > 0 {
                    sub_modes[(idy * 2) as usize]
                } else if avail_l {
                    // SAFETY: `avail_l` guarantees `(row, col - 1)` is in strip bounds.
                    unsafe { self.mi_grid.get_unchecked(row, col - 1) }.sub_modes()
                        [(1 + idy * 2) as usize]
                } else {
                    DC_PRED
                };
                r.read_tree(&INTRA_MODE_TREE, |node| {
                    KF_Y_MODE_PROBS[above_mode as usize][left_mode as usize][node]
                }) as u8
            })
        };

        let uv_mode = r.read_tree(&INTRA_MODE_TREE, |node| {
            KF_UV_MODE_PROBS[y_mode as usize][node]
        }) as u8;

        Ok(MiInfo::new_intra(
            skip, tx_size, mi_size, y_mode, uv_mode, sub_modes, segment_id,
        ))
    }

    // =========================================================================
    // Inter frame (`FrameIsIntra == 0`) mode info decoding (spec §6.4.11-6.4.20).
    // =========================================================================

    /// `inter_frame_mode_info( )` (spec §6.4.11).
    pub(super) fn inter_frame_mode_info(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        mi_size: u8,
        avail_u: bool,
        avail_l: bool,
    ) -> Result<(MiInfo, Option<super::BlockMvs>), TileError> {
        let left_ref_frame = if avail_l {
            // SAFETY: `avail_l` guarantees `(row, col - 1)` is in strip bounds.
            unsafe { self.mi_grid.get_unchecked(row, col - 1) }.ref_frame()
        } else {
            [INTRA_FRAME, REF_NONE]
        };
        let above_ref_frame = if avail_u {
            // SAFETY: `avail_u` guarantees `(row - 1, col)` is in strip bounds.
            unsafe { self.mi_grid.get_unchecked(row - 1, col) }.ref_frame()
        } else {
            [INTRA_FRAME, REF_NONE]
        };
        let neighbors = NeighborRefInfo {
            avail_u,
            avail_l,
            left_ref_frame,
            above_ref_frame,
            left_intra: left_ref_frame[0] == INTRA_FRAME,
            above_intra: above_ref_frame[0] == INTRA_FRAME,
            left_single: left_ref_frame[1] == REF_NONE,
            above_single: above_ref_frame[1] == REF_NONE,
        };

        let segment_id = self.inter_segment_id(r, row, col, mi_size)?;
        let skip = self.read_skip(r, row, col, avail_u, avail_l, segment_id);
        let is_inter = self.read_is_inter(r, &neighbors, segment_id);
        let tx_size = self.read_tx_size(
            r,
            mi_size,
            !skip || !is_inter,
            (row, col),
            (avail_u, avail_l),
        );

        if is_inter {
            self.inter_block_mode_info(r, row, col, mi_size, tx_size, skip, segment_id, &neighbors)
        } else {
            Ok((
                self.intra_block_mode_info(r, mi_size, tx_size, skip, segment_id)?,
                None,
            ))
        }
    }

    /// `read_is_inter( )` (spec §6.4.13).
    fn read_is_inter(&mut self, r: &mut BoolDecoder, n: &NeighborRefInfo, segment_id: u8) -> bool {
        if self.seg_feature_active(segment_id, SEG_LVL_REF_FRAME) {
            return self.segmentation.feature_data[segment_id as usize][SEG_LVL_REF_FRAME]
                != INTRA_FRAME as i32;
        }
        let ctx = if n.avail_u && n.avail_l {
            if n.left_intra && n.above_intra {
                3
            } else {
                (n.left_intra || n.above_intra) as usize
            }
        } else if n.avail_u || n.avail_l {
            2 * (if n.avail_u {
                n.above_intra
            } else {
                n.left_intra
            } as usize)
        } else {
            0
        };
        let is_inter = r.read_bool(self.probs.is_inter_prob[ctx]);
        self.count(|c| c.is_inter[ctx][is_inter as usize] += 1);
        is_inter
    }

    /// `intra_block_mode_info( )` (spec §6.4.15). For intra blocks within an inter frame.
    fn intra_block_mode_info(
        &mut self,
        r: &mut BoolDecoder,
        mi_size: u8,
        tx_size: u8,
        skip: bool,
        segment_id: u8,
    ) -> Result<MiInfo, TileError> {
        let (y_mode, sub_modes) = if mi_size >= BLOCK_8X8 {
            let ctx = SIZE_GROUP_LOOKUP[mi_size as usize] as usize;
            let mode =
                r.read_tree(&INTRA_MODE_TREE, |node| self.probs.y_mode_probs[ctx][node]) as u8;
            self.count(|c| c.intra_mode[ctx][mode as usize] += 1);
            (mode, [mode; 4])
        } else {
            decode_sub8x8_intra_modes(mi_size, |_, _, _| {
                // sub_intra_mode: ctx is always 0 (spec §9.3.2).
                let mode =
                    r.read_tree(&INTRA_MODE_TREE, |node| self.probs.y_mode_probs[0][node]) as u8;
                self.count(|c| c.intra_mode[0][mode as usize] += 1);
                mode
            })
        };

        let uv_mode = r.read_tree(&INTRA_MODE_TREE, |node| {
            self.probs.uv_mode_probs[y_mode as usize][node]
        }) as u8;
        self.count(|c| c.uv_mode[y_mode as usize][uv_mode as usize] += 1);

        Ok(MiInfo::new_intra(
            skip, tx_size, mi_size, y_mode, uv_mode, sub_modes, segment_id,
        ))
    }

    /// `read_ref_frames( )` (spec §6.4.17).
    fn read_ref_frames(
        &mut self,
        r: &mut BoolDecoder,
        n: &NeighborRefInfo,
        segment_id: u8,
    ) -> [u8; 2] {
        if self.seg_feature_active(segment_id, SEG_LVL_REF_FRAME) {
            return [
                self.segmentation.feature_data[segment_id as usize][SEG_LVL_REF_FRAME] as u8,
                REF_NONE,
            ];
        }
        let comp_mode = if self.reference_mode == REFERENCE_MODE_SELECT {
            let ctx = self.comp_mode_ctx(n);
            let bit = r.read_bool(self.probs.comp_mode_prob[ctx]);
            self.count(|c| c.comp_mode[ctx][bit as usize] += 1);
            (bit as u8) + SINGLE_REFERENCE
        } else {
            self.reference_mode
        };

        if comp_mode == COMPOUND_REFERENCE {
            let idx = self.ref_frame_sign_bias[self.comp_fixed_ref as usize] as usize;
            let ctx = self.comp_ref_ctx(n);
            let comp_ref = r.read_bool(self.probs.comp_ref_prob[ctx]) as usize;
            self.count(|c| c.comp_ref[ctx][comp_ref] += 1);
            let mut ref_frame = [0u8; 2];
            ref_frame[idx] = self.comp_fixed_ref;
            ref_frame[1 - idx] = self.comp_var_ref[comp_ref];
            ref_frame
        } else {
            let ctx1 = self.single_ref_p1_ctx(n);
            let p1 = r.read_bool(self.probs.single_ref_prob[ctx1][0]);
            self.count(|c| c.single_ref[ctx1][0][p1 as usize] += 1);
            if p1 {
                let ctx2 = self.single_ref_p2_ctx(n);
                let p2 = r.read_bool(self.probs.single_ref_prob[ctx2][1]);
                self.count(|c| c.single_ref[ctx2][1][p2 as usize] += 1);
                [if p2 { ALTREF_FRAME } else { GOLDEN_FRAME }, REF_NONE]
            } else {
                [LAST_FRAME, REF_NONE]
            }
        }
    }

    /// Context derivation for `interp_filter` (spec §9.3.2). [`INTERP_FILTER_UNSET`] is a
    /// sentinel value meaning "at least one of the two neighboring blocks is intra, or the
    /// filters disagree".
    fn interp_filter_ctx(&self, row: u32, col: u32, n: &NeighborRefInfo) -> usize {
        let left_interp = if n.avail_l && n.left_ref_frame[0] > INTRA_FRAME {
            // SAFETY: `n.avail_l` guarantees `(row, col - 1)` is in strip bounds.
            unsafe { self.mi_grid.get_unchecked(row, col - 1) }.interp_filter()
        } else {
            INTERP_FILTER_UNSET
        };
        let above_interp = if n.avail_u && n.above_ref_frame[0] > INTRA_FRAME {
            // SAFETY: `n.avail_u` guarantees `(row - 1, col)` is in strip bounds.
            unsafe { self.mi_grid.get_unchecked(row - 1, col) }.interp_filter()
        } else {
            INTERP_FILTER_UNSET
        };
        if left_interp == above_interp || above_interp == INTERP_FILTER_UNSET {
            left_interp as usize
        } else if left_interp == INTERP_FILTER_UNSET {
            above_interp as usize
        } else {
            INTERP_FILTER_UNSET as usize
        }
    }

    /// Reads the per-block `interp_filter` syntax element when the frame filter is `SWITCHABLE`.
    fn read_interp_filter(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        n: &NeighborRefInfo,
    ) -> u8 {
        if self.interpolation_filter != SWITCHABLE {
            return self.interpolation_filter;
        }
        let ctx = self.interp_filter_ctx(row, col, n);
        let f = r.read_tree(&INTERP_FILTER_TREE, |node| {
            self.probs.interp_filter_probs[ctx][node]
        }) as u8;
        self.count(|c| c.interp_filter[ctx][f as usize] += 1);
        f
    }

    /// Reads one `inter_mode` symbol in `ctx`, updates counts, and returns `NEARESTMV + inter_mode`.
    #[inline]
    fn read_inter_y_mode(&mut self, r: &mut BoolDecoder, ctx: usize) -> u8 {
        let inter_mode = r.read_tree(&INTER_MODE_TREE, |node| {
            self.probs.inter_mode_probs[ctx][node]
        }) as u8;
        self.count(|c| c.inter_mode[ctx][inter_mode as usize] += 1);
        NEARESTMV + inter_mode
    }

    /// Decodes per-subblock inter modes and MVs for a sub-8x8 inter block (`mi_size < BLOCK_8X8`),
    /// returning `(last_y_mode, block_mvs)`.
    #[allow(clippy::too_many_arguments)]
    fn decode_sub8x8_inter_mvs(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        mi_size: u8,
        ref_frame: [u8; 2],
        n_refs: usize,
        mode_context: &[u8; 4],
        mut nearest_mv: [Mv; 2],
        mut near_mv: [Mv; 2],
        best_mv: [Mv; 2],
    ) -> (u8, [[[i32; 2]; 4]; 2]) {
        let mut block_mvs = [[[0i32; 2]; 4]; 2];
        let mut y_mode = ZEROMV;
        let num4x4w = NUM_4X4_BLOCKS_WIDE_LOOKUP[mi_size as usize] as u32;
        let num4x4h = NUM_4X4_BLOCKS_HIGH_LOOKUP[mi_size as usize] as u32;
        let ctx = mode_context[ref_frame[0] as usize] as usize;
        let mut idy = 0u32;
        while idy < 2 {
            let mut idx = 0u32;
            while idx < 2 {
                y_mode = self.read_inter_y_mode(r, ctx);
                let block = (idy * 2 + idx) as i32;
                if y_mode == NEARESTMV || y_mode == NEARMV {
                    for j in 0..n_refs {
                        let (nm, nr) = self.append_sub8x8_mvs(
                            row,
                            col,
                            mi_size,
                            block,
                            ref_frame[j],
                            j,
                            &block_mvs,
                        );
                        nearest_mv[j] = nm;
                        near_mv[j] = nr;
                    }
                }
                let mv = self.assign_mv(r, y_mode, n_refs, nearest_mv, near_mv, best_mv);
                for y2 in 0..num4x4h {
                    for x2 in 0..num4x4w {
                        let b = ((idy + y2) * 2 + idx + x2) as usize;
                        for (rl, block_mv) in mv.iter().enumerate().take(n_refs) {
                            block_mvs[rl][b] = *block_mv;
                        }
                    }
                }
                idx += num4x4w;
            }
            idy += num4x4h;
        }
        (y_mode, block_mvs)
    }

    /// `inter_block_mode_info( )` (spec §6.4.16).
    #[allow(clippy::too_many_arguments)]
    fn inter_block_mode_info(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        mi_size: u8,
        tx_size: u8,
        skip: bool,
        segment_id: u8,
        n: &NeighborRefInfo,
    ) -> Result<(MiInfo, Option<super::BlockMvs>), TileError> {
        let ref_frame = self.read_ref_frames(r, n, segment_id);

        let mut nearest_mv: [Mv; 2] = [ZERO_MV; 2];
        let mut near_mv: [Mv; 2] = [ZERO_MV; 2];
        let mut best_mv: [Mv; 2] = [ZERO_MV; 2];
        let mut mode_context = [0u8; 4];

        for j in 0..2 {
            if ref_frame[j] > INTRA_FRAME {
                let (ref_list_mv, ctx) = self.find_mv_refs(row, col, mi_size, ref_frame[j], -1);
                mode_context[ref_frame[j] as usize] = ctx;
                let (nearest, near, best) = self.find_best_ref_mvs(row, col, mi_size, ref_list_mv);
                nearest_mv[j] = nearest;
                near_mv[j] = near;
                best_mv[j] = best;
            }
        }

        let is_compound = ref_frame[1] > INTRA_FRAME;
        let n_refs = 1 + is_compound as usize;

        // §6.4.16: when seg_feature_active(SEG_LVL_SKIP), y_mode is forced to ZEROMV without
        // reading inter_mode. Bitstream conformance guarantees MiSize >= BLOCK_8X8 whenever
        // seg_feature_active(SEG_LVL_SKIP) is set here.
        let mut y_mode = ZEROMV;
        if !self.seg_feature_active(segment_id, SEG_LVL_SKIP) && mi_size >= BLOCK_8X8 {
            y_mode = self.read_inter_y_mode(r, mode_context[ref_frame[0] as usize] as usize);
        }

        let interp_filter = self.read_interp_filter(r, row, col, n);

        let (mv, block_mvs) = if mi_size < BLOCK_8X8 {
            let (sub_y_mode, mvs) = self.decode_sub8x8_inter_mvs(
                r,
                row,
                col,
                mi_size,
                ref_frame,
                n_refs,
                &mode_context,
                nearest_mv,
                near_mv,
                best_mv,
            );
            y_mode = sub_y_mode;
            ([mvs[0][3], mvs[1][3]], Some(mvs))
        } else {
            (
                self.assign_mv(r, y_mode, n_refs, nearest_mv, near_mv, best_mv),
                None,
            )
        };

        Ok((
            MiInfo::new_inter(
                skip,
                tx_size,
                mi_size,
                y_mode,
                segment_id,
                ref_frame,
                mv,
                interp_filter,
            ),
            block_mvs,
        ))
    }

    /// `assign_mv( isCompound )` (spec §6.4.18).
    fn assign_mv(
        &mut self,
        r: &mut BoolDecoder,
        y_mode: u8,
        n_refs: usize,
        nearest_mv: [Mv; 2],
        near_mv: [Mv; 2],
        best_mv: [Mv; 2],
    ) -> [Mv; 2] {
        let mut mv = [ZERO_MV; 2];
        for (i, slot) in mv.iter_mut().enumerate().take(n_refs) {
            *slot = match y_mode {
                NEWMV => self.read_mv(r, best_mv[i]),
                NEARESTMV => nearest_mv[i],
                NEARMV => near_mv[i],
                _ => ZERO_MV, // ZEROMV
            };
        }
        mv
    }

    /// `read_mv( ref )` (spec §6.4.19).
    fn read_mv(&mut self, r: &mut BoolDecoder, best_mv: Mv) -> Mv {
        let use_hp = self.allow_high_precision_mv && use_mv_hp(best_mv);
        let mv_joint = r.read_tree(&MV_JOINT_TREE, |node| self.probs.mv_joint_probs[node]) as u8;
        self.count(|c| c.mv_joint[mv_joint as usize] += 1);
        let mut diff = ZERO_MV;
        if mv_joint == MV_JOINT_HZVNZ || mv_joint == MV_JOINT_HNZVNZ {
            diff[0] = self.read_mv_component(r, 0, use_hp);
        }
        if mv_joint == MV_JOINT_HNZVZ || mv_joint == MV_JOINT_HNZVNZ {
            diff[1] = self.read_mv_component(r, 1, use_hp);
        }
        [best_mv[0] + diff[0], best_mv[1] + diff[1]]
    }

    /// `read_mv_component( comp )` (spec §6.4.20).
    fn read_mv_component(&mut self, r: &mut BoolDecoder, comp: usize, use_hp: bool) -> i32 {
        let sign = r.read_bool(self.probs.mv_sign_prob[comp]);
        self.count(|c| c.mv_sign[comp][sign as usize] += 1);
        let mv_class =
            r.read_tree(&MV_CLASS_TREE, |node| self.probs.mv_class_probs[comp][node]) as usize;
        self.count(|c| c.mv_class[comp][mv_class] += 1);
        let mag: u32 = if mv_class == 0 {
            let class0_bit = r.read_bool(self.probs.mv_class0_bit_prob[comp]) as u32;
            let class0_fr = r.read_tree(&MV_FR_TREE, |node| {
                self.probs.mv_class0_fr_probs[comp][class0_bit as usize][node]
            }) as u32;
            let class0_hp = if use_hp {
                r.read_bool(self.probs.mv_class0_hp_prob[comp]) as u32
            } else {
                1
            };
            self.count(|c| {
                c.mv_class0_bit[comp][class0_bit as usize] += 1;
                c.mv_class0_fr[comp][class0_bit as usize][class0_fr as usize] += 1;
                c.mv_class0_hp[comp][class0_hp as usize] += 1;
            });
            ((class0_bit << 3) | (class0_fr << 1) | class0_hp) + 1
        } else {
            let mut d: u32 = 0;
            for i in 0..mv_class {
                let bit = r.read_bool(self.probs.mv_bits_prob[comp][i]) as u32;
                self.count(|c| c.mv_bits[comp][i][bit as usize] += 1);
                d |= bit << i;
            }
            let fr = r.read_tree(&MV_FR_TREE, |node| self.probs.mv_fr_probs[comp][node]) as u32;
            let hp = if use_hp {
                r.read_bool(self.probs.mv_hp_prob[comp]) as u32
            } else {
                1
            };
            self.count(|c| {
                c.mv_fr[comp][fr as usize] += 1;
                c.mv_hp[comp][hp as usize] += 1;
            });
            (2u32 << (mv_class + 2)) + ((d << 3) | (fr << 1) | hp) + 1
        };
        if sign {
            -(mag as i32)
        } else {
            mag as i32
        }
    }
}
