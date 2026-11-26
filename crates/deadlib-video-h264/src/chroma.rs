//! Chroma macroblock decoding shared by the CAVLC and CABAC paths: inter
//! prediction of a partition, residual parsing, and reconstruction.

use crate::bitstream::BitstreamReader;
use crate::cabac::CabacReader;
use crate::cavlc::parse_residual_block_cavlc;
use crate::dpb::DecodedPicture;
use crate::error::DecodeError;
use crate::neighbor::{cabac_neighbor_nz_chroma, compute_nc_chroma, dequant_4x4_ac_raster};
use crate::pixel::Pixel;
use crate::residual::{
    chroma_dc_422, chroma_qp_bd, dequant_chroma_dc, inverse_dct_4x4, inverse_hadamard_2x2,
    ZIGZAG_4X4, ZIGZAG_4X4_FIELD,
};
use crate::slice_context::{SliceContext, SliceParams};

/// One list's motion for a partition: the reference picture, the ref_idx as
/// coded (field units in an MBAFF field MB, for weights and field choice)
/// and the motion vector.
pub(crate) type PartMotion<'r> = (&'r DecodedPicture, i8, [i16; 2]);

impl<P: Pixel> SliceContext<'_, P> {
    /// Chroma inter prediction of the partition at luma offset `(x, y)`,
    /// size `w` x `h`, of the current MB, written to both chroma planes:
    /// motion compensation from each list used, then weighted (explicit
    /// weights for one list, any weights or the average for two).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn predict_chroma_part(
        &mut self,
        mb_idx: usize,
        mb_x: usize,
        mb_y: usize,
        sp: &SliceParams,
        (x, y, w, h): (usize, usize, usize, usize),
        l0: Option<PartMotion<'_>>,
        l1: Option<PartMotion<'_>>,
    ) {
        let sy = self.chroma_shift_y();
        let c422 = self.chroma_422();
        let (cw, ch) = (w / 2, h >> sy);
        let (cx, cy) = (mb_x / 2 + x / 2, y >> sy);
        let n = cw * ch;
        if n == 0 || n > 128 {
            return;
        }
        if l0.is_none() && l1.is_none() {
            return;
        }
        let weights = sp
            .wctx
            .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx);
        // Per list: reference, chroma row stride and offset, MC origin row
        // and vertical MV. Vertical chroma MVs are in 1/8 sample units for
        // 4:2:0 (with the field parity offset), 1/4 for 4:2:2 (spec 8.4.1.4,
        // 8.4.2.2.2): doubled, they use the same filter.
        let setup = |(pic, ref_idx, mv): PartMotion<'_>| {
            let (_, _, _, mc_cy, c_stride, c_off) =
                self.mc_params(mb_idx, mb_y, pic.width as usize, ref_idx);
            let mv_y = if c422 {
                mv[1] as i32 * 2
            } else {
                mv[1] as i32 + self.chroma_mv_offset(pic, c_off)
            };
            (c_stride, c_off, mc_cy + cy, mv[0] as i32, mv_y)
        };
        let m0 = l0.map(|m| (m.0, setup(m)));
        let m1 = l1.map(|m| (m.0, setup(m)));
        let ref_h = (self.height >> sy) as usize;
        type McSetup<'r> = (&'r DecodedPicture, (usize, usize, usize, i32, i32));
        let mc =
            |(pic, (c_stride, c_off, y0, mvx, mvy)): McSetup<'_>, plane: usize, out: &mut [P]| {
                P::chroma_mc(
                    pic.chroma_ref(plane, c_off),
                    c_stride,
                    ref_h,
                    cx as i32,
                    y0 as i32,
                    mvx,
                    mvy,
                    cw,
                    ch,
                    out,
                );
            };
        let mut pred = [P::default(); 128];
        for plane in 0..2 {
            match (m0, m1) {
                (Some(a), Some(b)) => {
                    let mut p0 = [P::default(); 128];
                    let mut p1 = [P::default(); 128];
                    mc(a, plane, &mut p0[..n]);
                    mc(b, plane, &mut p1[..n]);
                    weights.apply_bi(
                        &p0[..n],
                        &p1[..n],
                        &mut pred[..n],
                        l0.map_or(0, |m| m.1) as usize,
                        l1.map_or(0, |m| m.1) as usize,
                        true,
                        plane,
                    );
                }
                (Some(a), None) | (None, Some(a)) => {
                    mc(a, plane, &mut pred[..n]);
                    if sp.use_weight == 1 {
                        let (list, ref_idx) = match l0 {
                            Some(m) => (0, m.1),
                            None => (1, l1.map_or(0, |m| m.1)),
                        };
                        weights.apply_uni(&mut pred[..n], list, ref_idx as usize, true, plane);
                    }
                }
                (None, None) => return,
            }
            let dst = if plane == 0 {
                &mut self.frame.u
            } else {
                &mut self.frame.v
            };
            for r in 0..ch {
                let o = self.lc_offset + (cy + r) * self.lc_stride + cx;
                P::store_row(dst, o, &pred[r * cw..(r + 1) * cw]);
            }
        }
    }

    /// Chroma QP, scaling list and coefficient scan of `plane` for the
    /// residual of the current MB: lists 1/2 (intra) or 4/5 (inter); Cr
    /// takes the second chroma QP offset.
    fn chroma_params(
        &self,
        plane: usize,
        qp_y: i32,
        intra: bool,
        mb_idx: usize,
        sp: &SliceParams,
    ) -> (i32, [u8; 16], &'static [(usize, usize); 16]) {
        let offset = if plane == 0 {
            sp.chroma_qp_index_offset
        } else {
            sp.second_chroma_qp_index_offset
        };
        let scale = sp.scaling_list_4x4[if intra { 1 } else { 4 } + plane];
        let zigzag = if self.is_field_scan(mb_idx) {
            &ZIGZAG_4X4_FIELD
        } else {
            &ZIGZAG_4X4
        };
        // QP'C = QP_C + QpBdOffsetC
        let qp_c = chroma_qp_bd(qp_y, offset, P::QP_BD_OFFSET) + P::QP_BD_OFFSET;
        (qp_c, scale, zigzag)
    }

    /// Inverse transform and scaling of a plane's chroma DC (spec 8.5.11):
    /// `dc` in parse order becomes the DC of each 4x4 block, raster order.
    fn scale_chroma_dc(&self, dc: &mut [i32; 8], qp_c: i32, scale_dc: u8) {
        if self.chroma_422() {
            chroma_dc_422(dc, qp_c, scale_dc);
        } else {
            let mut dc4 = [dc[0], dc[1], dc[2], dc[3]];
            inverse_hadamard_2x2(&mut dc4);
            dequant_chroma_dc(&mut dc4, qp_c, scale_dc);
            dc[..4].copy_from_slice(&dc4);
        }
    }

    /// Reconstruct chroma 4x4 block `blk` of `plane`: `block` holds the
    /// scaled AC coefficients in raster order (`has_ac`), `dc` its DC; the
    /// inverse transform is added to the prediction in the frame.
    fn add_chroma_block(
        &mut self,
        plane: usize,
        blk: usize,
        mut block: [i32; 16],
        dc: i32,
        has_ac: bool,
        mb_x: usize,
    ) {
        if dc == 0 && !has_ac {
            // An all-zero block adds nothing
            return;
        }
        block[0] = dc;
        inverse_dct_4x4(&mut block);
        let dst = if plane == 0 {
            &mut self.frame.u
        } else {
            &mut self.frame.v
        };
        let (by, bx) = ((blk / 2) * 4, (blk % 2) * 4);
        let o = self.lc_offset + by * self.lc_stride + mb_x / 2 + bx;
        crate::residual::add_residual::<P>(dst, o, self.lc_stride, &block, 4, (4, 4));
    }

    /// Parse the chroma residual of a CAVLC macroblock (spec 7.3.5.3) and
    /// add it to the prediction already in the frame, block by block.
    /// Records each AC block's total_coeff for later nC derivation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn decode_chroma_residual_cavlc(
        &mut self,
        reader: &mut BitstreamReader,
        mb_idx: usize,
        mb_x: usize,
        cbp_chroma: u8,
        qp_y: i32,
        intra: bool,
        sp: &SliceParams,
    ) -> Result<(), DecodeError> {
        if cbp_chroma == 0 {
            return Ok(());
        }
        // 4 chroma blocks per plane for 4:2:0 (DC nC -1), 8 for 4:2:2 (-2)
        let (blocks, rows, dc_nc) = if self.chroma_422() {
            (8, 4, -2)
        } else {
            (4, 2, -1)
        };
        let mut dc = [[0i32; 8]; 2];
        for d in &mut dc {
            parse_residual_block_cavlc(reader, &mut d[..blocks], blocks, dc_nc)?;
        }
        for (plane, dc) in dc.iter_mut().enumerate() {
            let (qp_c, scale, zigzag) = self.chroma_params(plane, qp_y, intra, mb_idx, sp);
            self.scale_chroma_dc(dc, qp_c, scale[0]);
            for (blk, &dc) in dc.iter().enumerate().take(blocks) {
                let mut block = [0i32; 16];
                let mut has_ac = false;
                if cbp_chroma >= 2 {
                    let nc_arr: &[u8] = if plane == 0 { self.nc_cb } else { self.nc_cr };
                    let nc = compute_nc_chroma(
                        nc_arr,
                        mb_idx,
                        self.mb_width as usize,
                        blk,
                        rows,
                        self.mb_slice_id,
                        self.this_slice_id,
                        self.mbaff,
                        self.mb_field_decoding,
                    );
                    let mut ac = [0i32; 15];
                    let tc = parse_residual_block_cavlc(reader, &mut ac, 15, nc)?;
                    let nc_arr = if plane == 0 {
                        &mut *self.nc_cb
                    } else {
                        &mut *self.nc_cr
                    };
                    nc_arr[mb_idx * 8 + blk] = tc;
                    if tc > 0 {
                        for (i, &v) in ac.iter().enumerate() {
                            let (r, c) = zigzag[i + 1];
                            block[r * 4 + c] = v;
                        }
                        dequant_4x4_ac_raster(&mut block, qp_c, &scale);
                        has_ac = true;
                    }
                }
                self.add_chroma_block(plane, blk, block, dc, has_ac, mb_x);
            }
        }
        Ok(())
    }

    /// Parse the chroma residual of a CABAC macroblock (spec 7.3.5.3), with
    /// coded_block_flag contexts from the neighbours (an unavailable
    /// neighbour counts as coded for an intra macroblock), and add it to
    /// the prediction already in the frame, block by block.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn decode_chroma_residual_cabac(
        &mut self,
        cr: &mut CabacReader,
        st: &mut [u8; 1024],
        mb_idx: usize,
        mb_x: usize,
        cbp_chroma: u8,
        qp_y: i32,
        intra: bool,
        sp: &SliceParams,
    ) {
        if cbp_chroma == 0 {
            return;
        }
        let field = self.is_field_coded(mb_idx);
        let (blocks, rows) = if self.chroma_422() { (8, 4) } else { (4, 2) };
        let mut dc = [[0i32; 8]; 2];
        for (plane, d) in dc.iter_mut().enumerate() {
            let bit = 6 + plane;
            let left = self
                .left_mb(mb_idx)
                .map_or(intra, |m| (self.mb_cbp[m] >> bit) & 1 != 0);
            let top = self
                .above_mb(mb_idx)
                .map_or(intra, |m| (self.mb_cbp[m] >> bit) & 1 != 0);
            if cr.decode_coded_block_flag(st, 3, left, top) {
                let (coeffs, _) = cr.decode_residual_cabac_field(st, 3, blocks, field);
                for &(p, v) in coeffs {
                    d[p] = v;
                }
                self.mb_cbp[mb_idx] |= 1 << bit;
            }
        }
        for (plane, dc) in dc.iter_mut().enumerate() {
            let (qp_c, scale, zigzag) = self.chroma_params(plane, qp_y, intra, mb_idx, sp);
            self.scale_chroma_dc(dc, qp_c, scale[0]);
            for (blk, &dc) in dc.iter().enumerate().take(blocks) {
                let mut block = [0i32; 16];
                let mut has_ac = false;
                if cbp_chroma >= 2 {
                    let nc_arr: &[u8] = if plane == 0 { self.nc_cb } else { self.nc_cr };
                    let nz = |is_left: bool| {
                        cabac_neighbor_nz_chroma(
                            nc_arr,
                            mb_idx,
                            self.mb_width as usize,
                            blk,
                            is_left,
                            intra,
                            rows,
                            self.mb_slice_id,
                            self.this_slice_id,
                            self.mbaff,
                            self.mb_field_decoding,
                        )
                    };
                    let (left, top) = (nz(true), nz(false));
                    if cr.decode_coded_block_flag(st, 4, left, top) {
                        let (coeffs, tc) = cr.decode_residual_cabac_field(st, 4, 15, field);
                        let nc_arr = if plane == 0 {
                            &mut *self.nc_cb
                        } else {
                            &mut *self.nc_cr
                        };
                        nc_arr[mb_idx * 8 + blk] = tc;
                        for &(p, v) in coeffs {
                            let (r, c) = zigzag[p + 1];
                            block[r * 4 + c] = v;
                        }
                        dequant_4x4_ac_raster(&mut block, qp_c, &scale);
                        has_ac = true;
                    }
                }
                self.add_chroma_block(plane, blk, block, dc, has_ac, mb_x);
            }
        }
    }
}
