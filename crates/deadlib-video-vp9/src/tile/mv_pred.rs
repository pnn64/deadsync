//! Motion vector prediction (spec §6.5 "Motion vector prediction"):
//! `find_mv_refs`/`find_best_ref_mvs`/`append_sub8x8_mvs` and their neighbor-scanning helpers.
//!
//! Also absorbs the former standalone `src/mv.rs` module (W5): its pure helpers (clamping,
//! sign inversion, threshold checks) were factored out on their own specifically because
//! `find_mv_refs`/`find_best_ref_mvs`/`append_sub8x8_mvs` had to stay in `tile.rs` (borrow
//! convenience with `TileDecoder`/`MiGrid`); now that those methods live in this
//! `tile`-submodule alongside them, keeping the pure helpers in a separate top-level module
//! serves no purpose.

use super::{MiGrid, MiInfo, TileDecoder};
use crate::common::INTRA_FRAME;
use crate::mv_ref_tables::{
    COUNTER_TO_CONTEXT, IDX_N_COLUMN_TO_SUBBLOCK, MODE_2_COUNTER, MV_REF_BLOCKS,
};
use crate::prob_tables::{NUM_8X8_BLOCKS_HIGH_LOOKUP, NUM_8X8_BLOCKS_WIDE_LOOKUP};

/// A motion vector (`[row, col]`, in units of 1/8 pel).
pub type Mv = [i32; 2];

/// `ZeroMv` (spec §6.4.18 and others). Used from `tile::mode_info` too (`assign_mv`/
/// `inter_block_mode_info`), hence `pub(super)`.
pub(super) const ZERO_MV: Mv = [0, 0];

/// `MVREF_NEIGHBOURS` (spec §3, constants list). The number of neighbor
/// candidates `find_mv_refs` scans.
const MVREF_NEIGHBOURS: usize = 8;

/// `MV_BORDER` (spec §3). The clamp border used by `clamp_mv_ref` at the end
/// of `find_mv_refs`.
const MV_BORDER: i32 = 128;

/// `(BORDERINPIXELS - INTERP_EXTEND) << 3` (spec §6.5.12). The clamp border
/// used by `find_best_ref_mvs` (`BORDERINPIXELS = 160`, `INTERP_EXTEND = 4`).
const MV_PRED_BORDER: i32 = (160 - 4) << 3;

/// `COMPANDED_MVREF_THRESH` (spec §3). The threshold used by `use_mv_hp`.
const COMPANDED_MVREF_THRESH: i32 = 8;

/// `MI_SIZE` (spec §3). The side length in pixels of an 8x8 mode info unit.
const MI_SIZE: i32 = 8;

/// Shared body of `clamp_mv_row` (spec §6.5.4) and `clamp_mv_col` (spec §6.5.5).
#[inline]
fn clamp_mv_coord(mvec: i32, border: i32, mi_pos: u32, block_mis: u32, mi_limit: u32) -> i32 {
    let mb_to_near = -((mi_pos as i32) * MI_SIZE * 8);
    let mb_to_far = ((mi_limit as i32) - (block_mis as i32) - (mi_pos as i32)) * MI_SIZE * 8;
    mvec.clamp(mb_to_near - border, mb_to_far + border)
}

/// `add_mv_ref_list( refList )` (spec §6.5.6). Handles appending to
/// `RefListMv`/`RefMvCount`, including deduplication (a value equal to
/// `RefListMv[0]` is not added) and capping at a maximum of 2 entries.
fn add_mv_ref_list(ref_list_mv: &mut [Mv; 2], ref_mv_count: &mut usize, candidate: Mv) {
    if *ref_mv_count >= 2 {
        return;
    }
    if *ref_mv_count > 0 && candidate == ref_list_mv[0] {
        return;
    }
    ref_list_mv[*ref_mv_count] = candidate;
    *ref_mv_count += 1;
}

/// `scale_mv( refList, refFrame )` (spec §6.5.9). Flips the sign of the
/// motion vector if `ref_frame_sign_bias` differs between the candidate
/// frame and the target reference frame.
fn scale_mv(
    candidate_mv: Mv,
    cand_frame: u8,
    ref_frame: u8,
    ref_frame_sign_bias: &[bool; 4],
) -> Mv {
    if ref_frame_sign_bias[cand_frame as usize] != ref_frame_sign_bias[ref_frame as usize] {
        [-candidate_mv[0], -candidate_mv[1]]
    } else {
        candidate_mv
    }
}

/// `use_mv_hp( deltaMv )` (spec §6.5.13). Used from `tile::mode_info` too (`read_mv`), hence
/// `pub(super)`.
pub(super) fn use_mv_hp(delta_mv: Mv) -> bool {
    (delta_mv[0].abs() >> 3) < COMPANDED_MVREF_THRESH
        && (delta_mv[1].abs() >> 3) < COMPANDED_MVREF_THRESH
}

impl TileDecoder<'_> {
    /// `is_inside( candidateR, candidateC )` (spec §6.5.2).
    fn is_inside(&self, r: i32, c: i32) -> bool {
        r >= 0
            && (r as u32) < self.mi_rows
            && c >= self.mi_col_start as i32
            && c < self.mi_col_end as i32
    }

    /// Borrows `prev_mi_grid` when `use_prev_frame_mvs` is active and a previous grid is present.
    #[inline]
    fn prev_mi_grid_if_enabled(&self) -> Option<&MiGrid> {
        if self.use_prev_frame_mvs {
            self.prev_mi_grid.as_deref()
        } else {
            None
        }
    }

    /// `if_same_ref_frame_add_mv( candidateR, candidateC, refFrame, usePrev )` (spec §6.5.7).
    #[inline]
    fn add_same_ref_frame_mv(
        info: &MiInfo,
        ref_frame: u8,
        ref_list_mv: &mut [Mv; 2],
        ref_mv_count: &mut usize,
    ) {
        let rf = info.ref_frame();
        if rf[0] == ref_frame {
            add_mv_ref_list(ref_list_mv, ref_mv_count, info.mv[0]);
        } else if rf[1] == ref_frame {
            add_mv_ref_list(ref_list_mv, ref_mv_count, info.mv[1]);
        }
    }

    /// `if_diff_ref_frame_add_mv( candidateR, candidateC, refFrame, usePrev )` (spec §6.5.8).
    #[inline]
    fn add_diff_ref_frame_mv(
        &self,
        info: &MiInfo,
        ref_frame: u8,
        ref_list_mv: &mut [Mv; 2],
        ref_mv_count: &mut usize,
    ) {
        let [frame0, frame1] = info.ref_frame();
        let mv0 = info.mv[0];
        let mv1 = info.mv[1];
        let mvs_same = mv0 == mv1;
        if frame0 > INTRA_FRAME && frame0 != ref_frame {
            let scaled = scale_mv(mv0, frame0, ref_frame, &self.ref_frame_sign_bias);
            add_mv_ref_list(ref_list_mv, ref_mv_count, scaled);
        }
        if frame1 > INTRA_FRAME && frame1 != ref_frame && !mvs_same {
            let scaled = scale_mv(mv1, frame1, ref_frame, &self.ref_frame_sign_bias);
            add_mv_ref_list(ref_list_mv, ref_mv_count, scaled);
        }
    }

    /// `find_mv_refs( refFrame, block )` (spec §6.5.1). The return value is `(RefListMv, ModeContext)`.
    pub(super) fn find_mv_refs(
        &self,
        row: u32,
        col: u32,
        mi_size: u8,
        ref_frame: u8,
        block: i32,
    ) -> ([Mv; 2], u8) {
        let mut ref_list_mv = [ZERO_MV; 2];
        let mut ref_mv_count = 0usize;
        let mut different_ref_found = false;
        let mut context_counter: u32 = 0;

        let search = &MV_REF_BLOCKS[mi_size as usize];

        for &(dr, dc) in search.iter().take(2) {
            let cr = row as i32 + dr;
            let cc = col as i32 + dc;
            if self.is_inside(cr, cc) {
                different_ref_found = true;
                // SAFETY: `is_inside(cr, cc)` guarantees `0 <= cr < mi_rows` and
                // `mi_col_start <= cc < mi_col_end`, inside `self.mi_grid`'s strip.
                let cand = unsafe { self.mi_grid.get_unchecked(cr as u32, cc as u32) };
                context_counter += MODE_2_COUNTER[cand.y_mode() as usize] as u32;
                let cand_rf = cand.ref_frame();
                for (j, &rf) in cand_rf.iter().enumerate() {
                    if rf == ref_frame {
                        let idx = if block >= 0 {
                            IDX_N_COLUMN_TO_SUBBLOCK[block as usize][(dc == 0) as usize] as usize
                        } else {
                            3
                        };
                        let cand_mv = self.mi_grid.get_sub_mv(cand, j, idx);
                        add_mv_ref_list(&mut ref_list_mv, &mut ref_mv_count, cand_mv);
                        break;
                    }
                }
            }
        }

        // Once `ref_mv_count == 2`, `add_mv_ref_list` is a no-op and `context_counter` is already
        // final (only the first 2 neighbors contribute to it), so all remaining spatial and
        // temporal (`prev_mi_grid`) lookups can be skipped without changing any output.
        if ref_mv_count < 2 {
            for &(dr, dc) in search.iter().skip(2).take(MVREF_NEIGHBOURS - 2) {
                let cr = row as i32 + dr;
                let cc = col as i32 + dc;
                if self.is_inside(cr, cc) {
                    different_ref_found = true;
                    // SAFETY: `is_inside(cr, cc)` guarantees `(cr, cc)` is within strip bounds.
                    let cand = unsafe { self.mi_grid.get_unchecked(cr as u32, cc as u32) };
                    Self::add_same_ref_frame_mv(
                        cand,
                        ref_frame,
                        &mut ref_list_mv,
                        &mut ref_mv_count,
                    );
                    if ref_mv_count >= 2 {
                        break;
                    }
                }
            }
        }

        if ref_mv_count < 2 {
            if let Some(prev) = self.prev_mi_grid_if_enabled() {
                // SAFETY: `use_prev_frame_mvs` requires matching frame dimensions, so
                // `(row, col)` is within `prev`'s whole-frame grid bounds.
                let cand = unsafe { prev.get_unchecked(row, col) };
                Self::add_same_ref_frame_mv(cand, ref_frame, &mut ref_list_mv, &mut ref_mv_count);
            }
        }

        if ref_mv_count < 2 && different_ref_found {
            for &(dr, dc) in search.iter().take(MVREF_NEIGHBOURS) {
                let cr = row as i32 + dr;
                let cc = col as i32 + dc;
                if self.is_inside(cr, cc) {
                    // SAFETY: `is_inside(cr, cc)` guarantees `(cr, cc)` is within strip bounds.
                    let cand = unsafe { self.mi_grid.get_unchecked(cr as u32, cc as u32) };
                    self.add_diff_ref_frame_mv(
                        cand,
                        ref_frame,
                        &mut ref_list_mv,
                        &mut ref_mv_count,
                    );
                    if ref_mv_count >= 2 {
                        break;
                    }
                }
            }
        }

        if ref_mv_count < 2 {
            if let Some(prev) = self.prev_mi_grid_if_enabled() {
                // SAFETY: `use_prev_frame_mvs` guarantees matching dimensions for `prev`.
                let cand = unsafe { prev.get_unchecked(row, col) };
                self.add_diff_ref_frame_mv(cand, ref_frame, &mut ref_list_mv, &mut ref_mv_count);
            }
        }

        let mode_context = COUNTER_TO_CONTEXT[context_counter.min(18) as usize];

        let bh = NUM_8X8_BLOCKS_HIGH_LOOKUP[mi_size as usize] as u32;
        let bw = NUM_8X8_BLOCKS_WIDE_LOOKUP[mi_size as usize] as u32;
        for mv in ref_list_mv.iter_mut() {
            mv[0] = clamp_mv_coord(mv[0], MV_BORDER, row, bh, self.mi_rows);
            mv[1] = clamp_mv_coord(mv[1], MV_BORDER, col, bw, self.mi_cols);
        }

        (ref_list_mv, mode_context)
    }

    /// `find_best_ref_mvs( refList )` (spec §6.5.12). The return value is `(NearestMv, NearMv, BestMv)`.
    pub(super) fn find_best_ref_mvs(
        &self,
        row: u32,
        col: u32,
        mi_size: u8,
        ref_list_mv: [Mv; 2],
    ) -> (Mv, Mv, Mv) {
        let bh = NUM_8X8_BLOCKS_HIGH_LOOKUP[mi_size as usize] as u32;
        let bw = NUM_8X8_BLOCKS_WIDE_LOOKUP[mi_size as usize] as u32;
        let mut out = ref_list_mv;
        for mv in out.iter_mut() {
            if !self.allow_high_precision_mv || !use_mv_hp(*mv) {
                for comp in mv.iter_mut() {
                    if *comp & 1 != 0 {
                        *comp += if *comp > 0 { -1 } else { 1 };
                    }
                }
            }
            mv[0] = clamp_mv_coord(mv[0], MV_PRED_BORDER, row, bh, self.mi_rows);
            mv[1] = clamp_mv_coord(mv[1], MV_PRED_BORDER, col, bw, self.mi_cols);
        }
        (out[0], out[1], out[0])
    }

    /// `append_sub8x8_mvs( block, refList )` (spec §6.5.14). The return value is `(NearestMv, NearMv)`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_sub8x8_mvs(
        &self,
        row: u32,
        col: u32,
        mi_size: u8,
        block: i32,
        ref_frame: u8,
        ref_list: usize,
        block_mvs: &[[Mv; 4]; 2],
    ) -> (Mv, Mv) {
        let (ref_list_mv, _ctx) = self.find_mv_refs(row, col, mi_size, ref_frame, block);
        // Fixed-size list (never holds more than 2 entries) + length counter, the same
        // pattern `find_mv_refs` uses via `add_mv_ref_list` -- avoids a per-block heap alloc.
        let mut sub8x8 = [ZERO_MV; 2];
        let mut len; // assigned in every arm below before any read.

        if block == 0 {
            // Unconditional, no dedup (unlike the `add_mv_ref_list` calls below).
            sub8x8[0] = ref_list_mv[0];
            sub8x8[1] = ref_list_mv[1];
            len = 2;
        } else if block <= 2 {
            sub8x8[0] = block_mvs[ref_list][0];
            len = 1;
        } else {
            sub8x8[0] = block_mvs[ref_list][2];
            len = 1;
            for &idx in &[1usize, 0] {
                add_mv_ref_list(&mut sub8x8, &mut len, block_mvs[ref_list][idx]);
            }
        }
        for &cand in ref_list_mv.iter().take(2) {
            add_mv_ref_list(&mut sub8x8, &mut len, cand);
        }
        if len < 2 {
            sub8x8[len] = ZERO_MV;
        }
        (sub8x8[0], sub8x8[1])
    }
}
