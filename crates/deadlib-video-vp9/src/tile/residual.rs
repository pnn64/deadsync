//! `residual()` (spec §6.4.21) and everything it calls directly: per-plane intra/inter
//! prediction dispatch, token reading (spec §6.4.24, §6.4.26), and inverse quantization +
//! inverse transform + reconstruction (spec §8.6.2). Also the per-frame dequant step table
//! builder (`build_dequant_table`, added W4b) since its only caller is `TileDecoder::new_with_prev`
//! and its only reason to exist is feeding this pipeline's dequantization step.

use super::{MiInfo, Mv, TileDecoder};
use crate::bool_coder::BoolDecoder;
use crate::common::{get_uv_tx_size as common_get_uv_tx_size, INTRA_FRAME};
use crate::header::{SegmentationParams, MAX_SEGMENTS};
use crate::predict::predict_intra as predict_intra_block;
use crate::predict::{predict_inter, InterPredictParams, RefPlaneView};
use crate::prob_tables::{
    coefband_8x8plus, mode2txfm_map, pareto, BLOCK_8X8, CAT_PROBS, COEFBAND_4X4, DCT_VAL_CATEGORY6,
    ENERGY_CLASS, EXTRA_BITS, LAST_FRAME, NUM_4X4_BLOCKS_HIGH_LOOKUP, NUM_4X4_BLOCKS_WIDE_LOOKUP,
    ONE_TOKEN, SS_SIZE_LOOKUP, TOKEN_TREE, TX_16X16, TX_32X32, TX_4X4, TX_8X8, ZERO_TOKEN,
};
use crate::quant::{get_ac_quant, get_dc_quant, get_qindex};
use crate::scan::{get_scan_and_nb, TxSize};
use crate::transform::{inverse_transform_block, TxType};

/// Builds the per-frame dequant step table, indexed `[segment_id][plane_kind][dc=0/ac=1]`
/// (`plane_kind`: 0 = luma, 1 = chroma -- `get_dc_quant`/`get_ac_quant` only distinguish
/// `plane == 0` from `plane != 0`, so chroma U and V share row 1).
///
/// `get_qindex`/`get_dc_quant`/`get_ac_quant` (spec §8.6.1) depend only on `segment_id` and
/// frame-level header values (`base_q_idx`, the `delta_q_*` fields, `bit_depth`) -- all fixed
/// for the whole frame -- so building this table once here (instead of re-deriving it on every
/// transform block) is exactly equivalent.
pub(super) fn build_dequant_table(
    segmentation: &SegmentationParams,
    base_q_idx: u8,
    bit_depth: u8,
    delta_q_y_dc: i32,
    delta_q_uv_dc: i32,
    delta_q_uv_ac: i32,
) -> [[[i64; 2]; 2]; MAX_SEGMENTS] {
    let mut table = [[[0i64; 2]; 2]; MAX_SEGMENTS];
    for (segment_id, seg_row) in table.iter_mut().enumerate() {
        let qindex = get_qindex(base_q_idx, segmentation, segment_id);
        for (plane_kind, dc_ac) in seg_row.iter_mut().enumerate() {
            let dc_quant = get_dc_quant(bit_depth, qindex, plane_kind, delta_q_y_dc, delta_q_uv_dc);
            let ac_quant = get_ac_quant(bit_depth, qindex, plane_kind, delta_q_uv_ac);
            *dc_ac = [dc_quant as i64, ac_quant as i64];
        }
    }
    table
}

impl TileDecoder<'_> {
    /// `get_uv_tx_size( )` (spec §6.4.22).
    fn get_uv_tx_size(&self, mi_size: u8, tx_size: u8) -> u8 {
        common_get_uv_tx_size(mi_size, tx_size, self.subsampling_x, self.subsampling_y)
    }

    /// `get_plane_block_size( subsize, plane )` (spec §6.4.23).
    fn get_plane_block_size(&self, subsize: u8, plane: usize) -> u8 {
        let subx = if plane > 0 { self.subsampling_x } else { 0 } as usize;
        let suby = if plane > 0 { self.subsampling_y } else { 0 } as usize;
        SS_SIZE_LOOKUP[subsize as usize][subx][suby]
    }

    /// `residual( )` (spec §6.4.21). The return value is `EobTotal` (spec §6.4.4).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn residual(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        info: &MiInfo,
        block_mvs: Option<&[[Mv; 4]; 2]>,
        avail_u: bool,
        avail_l: bool,
        is_inter: bool,
    ) -> u32 {
        let mi_size = info.mi_size();
        let tx_size = info.tx_size();
        let skip = info.skip();
        let y_mode = info.y_mode();
        let uv_mode = info.uv_mode();
        let sub_modes = info.sub_modes();
        let segment_id = info.segment_id();
        let bsize = mi_size.max(BLOCK_8X8);
        let mut eob_total = 0u32;

        for plane in 0..3usize {
            let tx_sz = if plane > 0 {
                self.get_uv_tx_size(mi_size, tx_size)
            } else {
                tx_size
            };
            let step = 1u32 << tx_sz;
            let plane_sz = self.get_plane_block_size(bsize, plane);
            let num4x4w = NUM_4X4_BLOCKS_WIDE_LOOKUP[plane_sz as usize] as u32;
            let num4x4h = NUM_4X4_BLOCKS_HIGH_LOOKUP[plane_sz as usize] as u32;
            let sub_x = if plane > 0 { self.subsampling_x } else { 0 };
            let sub_y = if plane > 0 { self.subsampling_y } else { 0 };
            let base_x = (col * 8) >> sub_x;
            let base_y = (row * 8) >> sub_y;
            let maxx = (self.mi_cols * 8) >> sub_x;
            let maxy = (self.mi_rows * 8) >> sub_y;
            let pred_max_x = ((self.mi_cols * 8) >> sub_x).saturating_sub(1) as usize;
            let pred_max_y = ((self.mi_rows * 8) >> sub_y).saturating_sub(1) as usize;

            if is_inter {
                self.predict_inter_for_plane(
                    plane, row, col, info, block_mvs, base_x, base_y, num4x4w, num4x4h,
                );
                if skip {
                    let bx = (base_x >> 2) as usize;
                    self.above_ctx.nonzero[plane][bx..bx + num4x4w as usize].fill(0);
                    let by = (base_y >> 2) as usize;
                    let left = &mut self.left_nonzero_context[plane];
                    for i in 0..num4x4h as usize {
                        left[(by + i) & 15] = 0;
                    }
                    continue;
                }
            }

            let mut block_idx = 0u32;
            let mut y = 0u32;
            while y < num4x4h {
                let mut x = 0u32;
                while x < num4x4w {
                    let start_x = base_x + 4 * x;
                    let start_y = base_y + 4 * y;
                    let mut nonzero = false;

                    if start_x < maxx && start_y < maxy {
                        if !is_inter {
                            let mode = if plane > 0 {
                                uv_mode
                            } else if mi_size >= BLOCK_8X8 {
                                y_mode
                            } else {
                                sub_modes[block_idx as usize]
                            };
                            let have_left = avail_l || x > 0;
                            let have_above = avail_u || y > 0;
                            let not_on_right = x + step < num4x4w;
                            predict_intra_block(
                                &mut self.planes[plane],
                                start_x as usize,
                                start_y as usize,
                                have_left,
                                have_above,
                                not_on_right,
                                tx_sz,
                                mode,
                                pred_max_x,
                                pred_max_y,
                                self.bit_depth,
                            );
                        }

                        if !skip {
                            let tx_type = self.compute_tx_type(
                                plane,
                                tx_sz,
                                mi_size,
                                y_mode,
                                &sub_modes,
                                block_idx as usize,
                                is_inter,
                            );
                            nonzero = self.tokens_and_reconstruct(
                                r,
                                plane,
                                start_x as usize,
                                start_y as usize,
                                tx_sz,
                                tx_type,
                                is_inter,
                                segment_id,
                            );
                        }
                    }

                    eob_total += nonzero as u32;
                    for i in 0..step {
                        self.above_ctx.nonzero[plane][((start_x >> 2) + i) as usize] =
                            nonzero as u8;
                        let left_idx = (((start_y >> 2) + i) % 16) as usize;
                        self.left_nonzero_context[plane][left_idx] = nonzero as u8;
                    }
                    block_idx += 1;
                    x += step;
                }
                y += step;
            }
        }
        eob_total
    }

    /// The inter-prediction step of `residual( )` for one plane: `predict_inter()`
    /// (spec §8.5.2, motion compensation / sub-pixel interpolation), including the per-4x4
    /// sub-block loop when `MiSize < BLOCK_8X8`. LANDMINE: the sub-8x8 chroma MV block index
    /// `(y * num4x4w + x)` is bit-exact-correct as written -- a plausible, spec-grounded
    /// "correction" to it once regressed the official 4:2:2 vector (see
    /// docs/implementation-notes.md); verify against the sweep before touching it.
    #[allow(clippy::too_many_arguments)]
    fn predict_inter_for_plane(
        &mut self,
        plane: usize,
        row: u32,
        col: u32,
        info: &MiInfo,
        block_mvs: Option<&[[Mv; 4]; 2]>,
        base_x: u32,
        base_y: u32,
        num4x4w: u32,
        num4x4h: u32,
    ) {
        let ref_frame = info.ref_frame();
        let mi_size = info.mi_size();
        let ref_views: [Option<RefPlaneView>; 2] = core::array::from_fn(|i| {
            let rf = ref_frame[i];
            if rf == INTRA_FRAME {
                return None;
            }
            let r = self.resolved_refs[(rf - LAST_FRAME) as usize].as_deref()?;
            Some(RefPlaneView {
                plane: match plane {
                    0 => &r.y,
                    1 => &r.u,
                    _ => &r.v,
                },
                width: r.width,
                height: r.height,
            })
        });
        let uniform_mvs = [[info.mv(0); 4], [info.mv(1); 4]];
        let inter_params = InterPredictParams {
            ref_frame,
            block_mvs: block_mvs.unwrap_or(&uniform_mvs),
            interp_filter: info.interp_filter(),
            mi_row: row,
            mi_col: col,
            mi_size,
            mi_rows: self.mi_rows,
            mi_cols: self.mi_cols,
            subsampling_x: self.subsampling_x,
            subsampling_y: self.subsampling_y,
            frame_width: self.frame_width,
            frame_height: self.frame_height,
            bit_depth: self.bit_depth,
            refs: [ref_views[0].as_ref(), ref_views[1].as_ref()],
        };

        if mi_size < BLOCK_8X8 {
            let mut y = 0u32;
            while y < num4x4h {
                let mut x = 0u32;
                while x < num4x4w {
                    let block_idx = (y * num4x4w + x) as usize;
                    predict_inter(
                        &mut self.planes[plane],
                        plane,
                        (base_x + 4 * x) as usize,
                        (base_y + 4 * y) as usize,
                        4,
                        4,
                        block_idx,
                        &inter_params,
                        &mut self.scratch.word_block,
                    );
                    x += 1;
                }
                y += 1;
            }
        } else {
            predict_inter(
                &mut self.planes[plane],
                plane,
                base_x as usize,
                base_y as usize,
                (num4x4w * 4) as usize,
                (num4x4h * 4) as usize,
                0,
                &inter_params,
                &mut self.scratch.word_block,
            );
        }
    }

    /// The part of `get_scan( )` (spec §6.4.25) that determines `TxType`.
    #[allow(clippy::too_many_arguments)]
    fn compute_tx_type(
        &self,
        plane: usize,
        tx_sz: u8,
        mi_size: u8,
        y_mode: u8,
        sub_modes: &[u8; 4],
        block_idx: usize,
        is_inter: bool,
    ) -> TxType {
        if plane > 0 || tx_sz == TX_32X32 {
            TxType::DctDct
        } else if tx_sz == TX_4X4 {
            if self.lossless || is_inter {
                TxType::DctDct
            } else {
                let mode = if mi_size < BLOCK_8X8 {
                    sub_modes[block_idx]
                } else {
                    y_mode
                };
                mode2txfm_map(mode)
            }
        } else {
            // When is_inter, y_mode takes NEARESTMV..NEWMV (10..13), and mode2txfm_map maps
            // all of these to DctDct (spec §10.2).
            mode2txfm_map(y_mode)
        }
    }

    fn tx_sz_to_scan_size(tx_sz: u8) -> TxSize {
        match tx_sz {
            TX_4X4 => TxSize::Tx4x4,
            TX_8X8 => TxSize::Tx8x8,
            TX_16X16 => TxSize::Tx16x16,
            _ => TxSize::Tx32x32,
        }
    }

    /// Decodes a token >= `TWO_TOKEN` from `TOKEN_TREE` starting at node 2 (after `ZERO_TOKEN`
    /// and `ONE_TOKEN` were ruled out in the hot loop).
    #[inline(never)]
    fn decode_large_token(r: &mut BoolDecoder, p2: u8) -> u8 {
        let mut n: i32 = 4; // node 2 in TOKEN_TREE
        loop {
            let node = (n as usize) >> 1;
            let bit = r.read_bool(pareto(node, p2)) as i32;
            n = TOKEN_TREE[(n + bit) as usize];
            if n <= 0 {
                break;
            }
        }
        (-n) as u8
    }

    /// Fast inline `TOKEN_TREE` decode: handles `ZERO_TOKEN` and `ONE_TOKEN` (~85-90% of all
    /// tokens in real video) directly without closure dispatch or `pareto` lookups.
    #[inline]
    fn decode_token(r: &mut BoolDecoder, probs: [u8; 3]) -> u8 {
        if !r.read_bool(probs[1]) {
            ZERO_TOKEN
        } else if !r.read_bool(probs[2]) {
            ONE_TOKEN
        } else {
            Self::decode_large_token(r, probs[2])
        }
    }

    /// Reconstructs a DC-only `DCT_DCT` transform block (`eob == 1`, `!lossless`): for every
    /// size `4x4..32x32` and all bit depths `8/10/12`, the 2D inverse DCT of `[dq0, 0, ...]`
    /// collapses to a uniform residual `round2(round2(round2(dq0 * 11585, 14) * 11585, 14), shift)`.
    #[allow(clippy::too_many_arguments)]
    fn reconstruct_dc_only(
        &mut self,
        plane: usize,
        start_x: usize,
        start_y: usize,
        n: u32,
        dq0: i64,
    ) {
        let n0 = 1usize << n;
        let shift = (n + 2).min(6);
        let v1 = (dq0 * 11585 + (1i64 << 13)) >> 14;
        let v2 = (v1 * 11585 + (1i64 << 13)) >> 14;
        let resid = (v2 + (1i64 << (shift - 1))) >> shift;
        let max_val = (1i64 << self.bit_depth) - 1;
        // Any delta outside [-max_val, max_val] saturates every valid pixel (0..=max_val) to
        // 0 or max_val, so clamping `delta` to [-max_val, max_val] is exact in i16 lanes too.
        let clamped_delta = resid.clamp(-max_val, max_val);

        #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
        if crate::simd::avx2_enabled() {
            let dst = &mut self.planes[plane];
            let stride = dst.stride();
            let local_x = start_x - dst.x0;
            macro_rules! add_dc {
                ($base:expr) => {
                    // SAFETY: AVX2 is enabled and the block fits in the padded plane.
                    unsafe {
                        crate::simd::reconstruct_add_const_avx2(
                            $base,
                            stride,
                            local_x,
                            start_y,
                            clamped_delta as i16,
                            n0,
                            max_val as i16,
                        )
                    }
                };
            }
            if let Some(base) = dst.byte_ptr() {
                add_dc!(base);
            } else {
                add_dc!(dst.word_ptr().unwrap());
            }
            return;
        }
        let delta = clamped_delta as i32;
        for row in 0..n0 {
            self.planes[plane].update_row(start_x, start_y + row, n0, |_, sample| {
                (i32::from(sample) + delta).clamp(0, max_val as i32) as u16
            });
        }
    }

    /// `tokens( )` (spec §6.4.24) + `reconstruct( )` (spec §8.6.2).
    /// The return value is `nonzero` (`nonzero = c > 0` from spec §6.4.24).
    #[allow(clippy::too_many_arguments)]
    fn tokens_and_reconstruct(
        &mut self,
        r: &mut BoolDecoder,
        plane: usize,
        start_x: usize,
        start_y: usize,
        tx_sz: u8,
        tx_type: TxType,
        is_inter: bool,
        segment_id: u8,
    ) -> bool {
        let n = (tx_sz as u32) + 2;
        let n0 = 1usize << n;
        let seg_eob = n0 * n0;
        let (scan, nb) = get_scan_and_nb(Self::tx_sz_to_scan_size(tx_sz), tx_type);

        let plane_type = if plane > 0 { 1usize } else { 0usize };
        let sub_x = if plane > 0 { self.subsampling_x } else { 0 };
        let sub_y = if plane > 0 { self.subsampling_y } else { 0 };
        let max_x_ctx = (2 * self.mi_cols) >> sub_x;
        let max_y_ctx = (2 * self.mi_rows) >> sub_y;
        let numpts = 1u32 << tx_sz;
        let x4 = (start_x >> 2) as u32;
        let y4 = (start_y >> 2) as u32;

        // The first more_coefs bit decides whether this block has any coefficient tokens at
        // all. Read it before touching the scratch buffers; an immediate EOB leaves prediction
        // unchanged.
        let first_ctx = {
            let mut above = 0u32;
            let mut left = 0u32;
            for i in 0..numpts {
                if x4 + i < max_x_ctx {
                    above |= self.above_ctx.nonzero[plane][(x4 + i) as usize] as u32;
                }
                if y4 + i < max_y_ctx {
                    left |= self.left_nonzero_context[plane][((y4 + i) % 16) as usize] as u32;
                }
            }
            (above + left) as usize
        };

        let first_probs =
            self.probs.coef_probs[tx_sz as usize][plane_type][is_inter as usize][0][first_ctx];
        let more_coefs = r.read_bool(first_probs[0]);
        self.count(|cnt| {
            cnt.more_coefs[tx_sz as usize][plane_type][is_inter as usize][0][first_ctx]
                [more_coefs as usize] += 1;
        });
        if !more_coefs {
            return false;
        }

        let is_32x32 = tx_sz == TX_32X32;
        let [dc_quant, ac_quant] = self.dequant_table[segment_id as usize][plane_type];

        // Preflight c = 0 (DC coefficient): if it is non-zero and followed immediately by EOB at
        // c = 1, a DCT_DCT block can take the 2-multiply DC-only path without zeroing either
        // scratch buffer or running a full 2D inverse transform.
        let tok0 = Self::decode_token(r, first_probs);
        self.count(|cnt| {
            cnt.token[tx_sz as usize][plane_type][is_inter as usize][0][first_ctx]
                [(tok0 as usize).min(2)] += 1;
        });
        let energy0 = ENERGY_CLASS[tok0 as usize];

        let dq0 = if tok0 == ZERO_TOKEN {
            0i64
        } else {
            let dq0 = self.dequant_nonzero_token(r, tok0, dc_quant, is_32x32);

            // At c = 1, band is always 1 and nb[1] is always [0, 0] for every scan table,
            // so ctx1 == (1 + 2 * energy0) >> 1 == energy0.
            let ctx1 = energy0 as usize;
            let p1 =
                self.probs.coef_probs[tx_sz as usize][plane_type][is_inter as usize][1][ctx1][0];
            let more_coefs_1 = r.read_bool(p1);
            self.count(|cnt| {
                cnt.more_coefs[tx_sz as usize][plane_type][is_inter as usize][1][ctx1]
                    [more_coefs_1 as usize] += 1;
            });
            if !more_coefs_1 {
                if tx_type == TxType::DctDct && !self.lossless {
                    self.reconstruct_dc_only(plane, start_x, start_y, n, dq0);
                    return true;
                }
                self.scratch.dequant_buf[..seg_eob].fill(0);
                self.scratch.dequant_buf[0] = dq0;
                self.transform_reconstruct(plane, start_x, start_y, tx_sz, tx_type, 0);
                return true;
            }
            dq0
        };

        // Non-DC-only block: zero only the active `..seg_eob` slice of our reusable scratch
        // buffers, seed c = 0, and continue from c = 1 (with check_eob == false because either
        // tok0 == ZERO_TOKEN or we already consumed more_coefs_1 == true above).
        self.scratch.dequant_buf[..seg_eob].fill(0);
        self.scratch.token_cache_buf[..seg_eob].fill(0);
        self.scratch.dequant_buf[0] = dq0;
        self.scratch.token_cache_buf[0] = energy0;

        let mut check_eob = false;
        let mut max_row = 0usize;
        let mut c = 1usize;
        let is_4x4 = tx_sz == TX_4X4;

        while c < seg_eob {
            let pos = scan[c] as usize;
            let band = if is_4x4 {
                COEFBAND_4X4[c] as usize
            } else {
                coefband_8x8plus(c) as usize
            };
            let [nb0, nb1] = nb[c];
            let ctx = ((1
                + self.scratch.token_cache_buf[nb0 as usize] as u32
                + self.scratch.token_cache_buf[nb1 as usize] as u32)
                >> 1) as usize;
            let probs =
                self.probs.coef_probs[tx_sz as usize][plane_type][is_inter as usize][band][ctx];

            if check_eob {
                // Counted ONLY when EOB is actually read (check_eob == true), matching spec §8.4.3.
                let more_coefs = r.read_bool(probs[0]);
                self.count(|cnt| {
                    cnt.more_coefs[tx_sz as usize][plane_type][is_inter as usize][band][ctx]
                        [more_coefs as usize] += 1;
                });
                if !more_coefs {
                    break;
                }
            }

            let token = Self::decode_token(r, probs);
            self.count(|cnt| {
                cnt.token[tx_sz as usize][plane_type][is_inter as usize][band][ctx]
                    [(token as usize).min(2)] += 1;
            });
            self.scratch.token_cache_buf[pos] = ENERGY_CLASS[token as usize];

            if token == ZERO_TOKEN {
                check_eob = false;
            } else {
                self.scratch.dequant_buf[pos] =
                    self.dequant_nonzero_token(r, token, ac_quant, is_32x32);
                max_row = max_row.max(pos >> n);
                check_eob = true;
            }

            c += 1;
        }

        self.transform_reconstruct(plane, start_x, start_y, tx_sz, tx_type, max_row);
        true
    }

    /// Reads the magnitude + sign for a non-zero coefficient token and applies dequantization.
    #[inline]
    fn dequant_nonzero_token(
        &self,
        r: &mut BoolDecoder,
        token: u8,
        quant: i64,
        is_32x32: bool,
    ) -> i64 {
        let coef = if token == ONE_TOKEN {
            1i32
        } else {
            self.read_coef(r, token)
        };
        let signed = if r.flag() { -coef } else { coef };
        let prod = signed as i64 * quant;
        if is_32x32 {
            prod / 2
        } else {
            prod
        }
    }

    /// Inverse transform + reconstruction (spec §8.6.2) of one transform block whose dequantized
    /// coefficients are already in `self.scratch.dequant_buf[..seg_eob]`.
    #[allow(clippy::too_many_arguments)]
    fn transform_reconstruct(
        &mut self,
        plane: usize,
        start_x: usize,
        start_y: usize,
        tx_sz: u8,
        tx_type: TxType,
        max_row: usize,
    ) {
        let n = (tx_sz as u32) + 2;
        let n0 = 1usize << n;
        let seg_eob = n0 * n0;

        let _ = max_row;
        #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
        if crate::simd::avx2_enabled() {
            let dequant = &self.scratch.dequant_buf[..seg_eob];
            let dst = &mut self.planes[plane];
            let stride = dst.stride();
            let local_x = start_x - dst.x0;
            macro_rules! reconstruct {
                ($base:expr) => {
                    // SAFETY: AVX2 is enabled. The coefficients and destination
                    // cover a complete transform block in the padded plane.
                    unsafe {
                        crate::simd::inverse_transform_reconstruct_avx2(
                            $base,
                            stride,
                            local_x,
                            start_y,
                            dequant,
                            n,
                            tx_type,
                            max_row,
                            self.lossless,
                            self.bit_depth,
                        )
                    }
                };
            }
            if let Some(base) = dst.byte_ptr() {
                reconstruct!(base);
            } else {
                reconstruct!(dst.word_ptr().unwrap());
            }
            return;
        }

        inverse_transform_block(
            &mut self.scratch.dequant_buf[..seg_eob],
            n,
            tx_type,
            self.lossless,
        );
        let max_val = (1i64 << self.bit_depth) - 1;
        for i in 0..n0 {
            let coeff = &self.scratch.dequant_buf[i * n0..][..n0];
            self.planes[plane].update_row(start_x, start_y + i, n0, |j, sample| {
                (i64::from(sample) + coeff[j]).clamp(0, max_val) as u16
            });
        }
    }

    /// `read_coef( token )` (spec §6.4.26).
    fn read_coef(&self, r: &mut BoolDecoder, token: u8) -> i32 {
        let row = &EXTRA_BITS[token as usize];
        let cat = row[0] as usize;
        let num_extra = row[1] as u32;
        let mut coef = row[2] as i32;

        if token == DCT_VAL_CATEGORY6 {
            // The cat6 high-bit loop (`for e in 0..(BitDepth-8)`): 0 iterations at 8-bit,
            // 2/4 at 10/12-bit (profiles 2/3), reading the extra magnitude bits above the
            // 14 base cat6 extra bits. Sweep-verified at all three bit depths.
            for e in 0..(self.bit_depth.saturating_sub(8) as u32) {
                let high_bit = r.read_bool(255) as i32;
                coef += high_bit << (5 + self.bit_depth as u32 - e);
            }
        }

        for e in 0..num_extra {
            let coef_bit = r.read_bool(CAT_PROBS[cat][e as usize]) as i32;
            coef += coef_bit << (num_extra - 1 - e);
        }

        coef
    }
}
