//! CAVLC macroblock decode for non-skip MBs.

use alloc::vec::Vec;

use crate::bitstream::BitstreamReader;
use crate::cavlc::parse_residual_block_cavlc;
use crate::deblock::{MbInfo, MbType};
use crate::error::DecodeError;
use crate::mv_pred::{predict_mv, predict_mv_sub, ref_pic_safe, MbaffCtx};
use crate::neighbor::{compute_nc, dequant_4x4_ac_raster, predict_i4x4_mode};
use crate::pixel::Pixel;
use crate::residual::{
    dequant_4x4_full, dequant_8x8, dequant_luma_dc_i16x16, inverse_dct_4x4, inverse_dct_8x8,
    inverse_hadamard_4x4, BLOCK_INDEX_TO_OFFSET, OFFSET_TO_BLOCK, ZIGZAG_4X4, ZIGZAG_4X4_FIELD,
    ZIGZAG_8X8_CAVLC, ZIGZAG_8X8_CAVLC_FIELD,
};
use crate::slice_context::{SliceContext, SliceParams};

impl<P: Pixel> SliceContext<'_, P> {
    /// Decode a single non-skip CAVLC macroblock (P inter, B inter, or intra).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::needless_range_loop)]
    pub(crate) fn decode_cavlc_mb(
        &mut self,
        reader: &mut BitstreamReader,
        mb_idx: usize,
        mb_x: usize,
        mb_y: usize,
        sp: &SliceParams,
    ) -> Result<(), DecodeError> {
        // Select field or frame coefficient scan tables
        let field_scan = self.is_field_scan(mb_idx);
        let zigzag_4x4 = if field_scan {
            &ZIGZAG_4X4_FIELD
        } else {
            &ZIGZAG_4X4
        };
        let zigzag_8x8_cavlc = if field_scan {
            &ZIGZAG_8X8_CAVLC_FIELD
        } else {
            &ZIGZAG_8X8_CAVLC
        };

        let raw_mb_type = reader.read_ue()?;
        // For P slices, mb_type >= 5 means intra (subtract 5)
        // For B slices, mb_type >= 23 means intra (subtract 23)
        let inter_limit = if sp.is_p_slice {
            5
        } else if sp.is_b_slice {
            23
        } else {
            0
        };
        let (mb_type, is_inter) = if (sp.is_p_slice || sp.is_b_slice) && raw_mb_type < inter_limit {
            (raw_mb_type, true)
        } else {
            (raw_mb_type - inter_limit, false)
        };
        if is_inter && sp.is_b_slice {
            // Field MBs in MBAFF address each frame reference as two fields
            let num_ref_l0 = self.effective_num_ref(mb_idx, sp.num_ref_idx_l0_active);
            let num_ref_l1 = self.effective_num_ref(mb_idx, sp.num_ref_idx_l1_active);
            // Reference list index of a (field) ref_idx; see `frame_ref_idx`
            let field_mb = self.mbaff && self.mb_field_decoding[mb_idx / 2];
            let fri = move |r: i8| if field_mb { r / 2 } else { r };
            // === Inter (B) macroblock ===
            // Table 7-11: mb_type 0=B_Direct_16x16, 1=B_L0_16x16,
            // 2=B_L1_16x16, 3=B_Bi_16x16, 4-21=16x8/8x16 variants, 22=B_8x8
            let mut no_sub_less_8x8_b = true; // for transform_size_8x8_flag
            struct SubPart {
                x: usize,
                y: usize,
                w: usize,
                h: usize,
                ref_idx_l0: i8,
                ref_idx_l1: i8,
                mv_l0: [i16; 2],
                mv_l1: [i16; 2],
                pred_l0: bool,
                pred_l1: bool,
            }
            let mut sub_parts: Vec<SubPart> = Vec::new();

            match mb_type {
                1 | 2 => {
                    // B_L0_16x16 (mb_type=1) or B_L1_16x16 (mb_type=2)
                    let is_l0 = mb_type == 1;
                    let num_active = if is_l0 { num_ref_l0 } else { num_ref_l1 };

                    let ref_idx = if num_active > 1 {
                        reader.read_te(num_active - 1)? as i8
                    } else {
                        0
                    };
                    let mvd_x = reader.read_se()? as i16;
                    let mvd_y = reader.read_se()? as i16;

                    // MV prediction using the appropriate store
                    let (mv_store_ref, ref_store_ref): (&[[i16; 2]], &[i8]) = if is_l0 {
                        (self.mv_store_l0, self.ref_idx_store_l0)
                    } else {
                        (self.mv_store_l1, self.ref_idx_store_l1)
                    };
                    let (mvp_x, mvp_y) = predict_mv(
                        mv_store_ref,
                        ref_store_ref,
                        mb_idx,
                        self.mb_width as usize,
                        0,
                        16,
                        16,
                        ref_idx,
                        self.mb_slice_id,
                        self.this_slice_id,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                    );
                    let mv = [mvp_x.wrapping_add(mvd_x), mvp_y.wrapping_add(mvd_y)];

                    // Store MV/ref for all 4x4 blocks
                    for blk in 0..16 {
                        if is_l0 {
                            self.mv_store_l0[mb_idx * 16 + blk] = mv;
                            self.ref_idx_store_l0[mb_idx * 16 + blk] = ref_idx;
                        } else {
                            self.mv_store_l1[mb_idx * 16 + blk] = mv;
                            self.ref_idx_store_l1[mb_idx * 16 + blk] = ref_idx;
                        }
                    }

                    sub_parts.push(SubPart {
                        x: 0,
                        y: 0,
                        w: 16,
                        h: 16,
                        ref_idx_l0: if is_l0 { ref_idx } else { -1 },
                        ref_idx_l1: if is_l0 { -1 } else { ref_idx },
                        mv_l0: if is_l0 { mv } else { [0, 0] },
                        mv_l1: if is_l0 { [0, 0] } else { mv },
                        pred_l0: is_l0,
                        pred_l1: !is_l0,
                    });
                }
                0 => {
                    // B_Direct_16x16: derive MVs per 4x4 block via spatial or temporal direct
                    if !sp.direct_8x8_inference_flag {
                        no_sub_less_8x8_b = false;
                    }
                    self.derive_direct_mvs(mb_idx, 0, 16, sp);
                    // Build sub_parts for MC, coalescing blocks with identical MVs/refs
                    // per 8x8 sub-block (spec 8.4.1.2.1: direct mode applied per 8x8)
                    let base = mb_idx * 16;
                    for i8x8 in 0..4 {
                        let blk0 = i8x8 * 4; // top-left 4x4 block of this 8x8
                        let mv0 = self.mv_store_l0[base + blk0];
                        let mv1 = self.mv_store_l1[base + blk0];
                        let r0 = self.ref_idx_store_l0[base + blk0];
                        let r1 = self.ref_idx_store_l1[base + blk0];
                        // Check if all 4 blocks in this 8x8 have the same MV/ref
                        let uniform = (1..4).all(|sub| {
                            let b = blk0 + sub;
                            self.mv_store_l0[base + b] == mv0
                                && self.mv_store_l1[base + b] == mv1
                                && self.ref_idx_store_l0[base + b] == r0
                                && self.ref_idx_store_l1[base + b] == r1
                        });
                        let (blk_row, blk_col) = BLOCK_INDEX_TO_OFFSET[blk0];
                        if uniform {
                            sub_parts.push(SubPart {
                                x: blk_col,
                                y: blk_row,
                                w: 8,
                                h: 8,
                                ref_idx_l0: r0,
                                ref_idx_l1: r1,
                                mv_l0: mv0,
                                mv_l1: mv1,
                                pred_l0: r0 >= 0,
                                pred_l1: r1 >= 0,
                            });
                        } else {
                            for sub in 0..4 {
                                let b = blk0 + sub;
                                let (br, bc) = BLOCK_INDEX_TO_OFFSET[b];
                                let rl0 = self.ref_idx_store_l0[base + b];
                                let rl1 = self.ref_idx_store_l1[base + b];
                                sub_parts.push(SubPart {
                                    x: bc,
                                    y: br,
                                    w: 4,
                                    h: 4,
                                    ref_idx_l0: rl0,
                                    ref_idx_l1: rl1,
                                    mv_l0: self.mv_store_l0[base + b],
                                    mv_l1: self.mv_store_l1[base + b],
                                    pred_l0: rl0 >= 0,
                                    pred_l1: rl1 >= 0,
                                });
                            }
                        }
                    }
                }
                3 => {
                    // B_Bi_16x16: both L0 and L1, averaged
                    let ref_idx_l0 = if num_ref_l0 > 1 {
                        reader.read_te(num_ref_l0 - 1)? as i8
                    } else {
                        0
                    };
                    let ref_idx_l1 = if num_ref_l1 > 1 {
                        reader.read_te(num_ref_l1 - 1)? as i8
                    } else {
                        0
                    };

                    let mvd_l0_x = reader.read_se()? as i16;
                    let mvd_l0_y = reader.read_se()? as i16;
                    let (mvp_l0_x, mvp_l0_y) = predict_mv(
                        self.mv_store_l0,
                        self.ref_idx_store_l0,
                        mb_idx,
                        self.mb_width as usize,
                        0,
                        16,
                        16,
                        ref_idx_l0,
                        self.mb_slice_id,
                        self.this_slice_id,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                    );
                    let mv_l0 = [
                        mvp_l0_x.wrapping_add(mvd_l0_x),
                        mvp_l0_y.wrapping_add(mvd_l0_y),
                    ];

                    let mvd_l1_x = reader.read_se()? as i16;
                    let mvd_l1_y = reader.read_se()? as i16;
                    let (mvp_l1_x, mvp_l1_y) = predict_mv(
                        self.mv_store_l1,
                        self.ref_idx_store_l1,
                        mb_idx,
                        self.mb_width as usize,
                        0,
                        16,
                        16,
                        ref_idx_l1,
                        self.mb_slice_id,
                        self.this_slice_id,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                    );
                    let mv_l1 = [
                        mvp_l1_x.wrapping_add(mvd_l1_x),
                        mvp_l1_y.wrapping_add(mvd_l1_y),
                    ];

                    // Store both L0 and L1 MVs
                    for blk in 0..16 {
                        self.mv_store_l0[mb_idx * 16 + blk] = mv_l0;
                        self.ref_idx_store_l0[mb_idx * 16 + blk] = ref_idx_l0;
                        self.mv_store_l1[mb_idx * 16 + blk] = mv_l1;
                        self.ref_idx_store_l1[mb_idx * 16 + blk] = ref_idx_l1;
                    }

                    sub_parts.push(SubPart {
                        x: 0,
                        y: 0,
                        w: 16,
                        h: 16,
                        ref_idx_l0,
                        ref_idx_l1,
                        mv_l0,
                        mv_l1,
                        pred_l0: true,
                        pred_l1: true,
                    });
                }
                4..=21 => {
                    // B 16x8/8x16 variants (Table 7-11)
                    // Each type specifies partition size and per-partition pred direction
                    // Format: (part_w, part_h, [(pred_l0_0, pred_l1_0), (pred_l0_1, pred_l1_1)])
                    #[rustfmt::skip]
                        #[allow(clippy::type_complexity)]
                        const B_PART_TABLE: [(usize, usize, [(bool, bool); 2]); 18] = [
                            // mb_type 4-5: B_L0_L0
                            (16, 8, [(true,false),(true,false)]),  // 4
                            (8, 16, [(true,false),(true,false)]),  // 5
                            // mb_type 6-7: B_L1_L1
                            (16, 8, [(false,true),(false,true)]), // 6
                            (8, 16, [(false,true),(false,true)]), // 7
                            // mb_type 8-9: B_L0_L1
                            (16, 8, [(true,false),(false,true)]), // 8
                            (8, 16, [(true,false),(false,true)]), // 9
                            // mb_type 10-11: B_L1_L0
                            (16, 8, [(false,true),(true,false)]), // 10
                            (8, 16, [(false,true),(true,false)]), // 11
                            // mb_type 12-13: B_L0_Bi
                            (16, 8, [(true,false),(true,true)]),  // 12
                            (8, 16, [(true,false),(true,true)]),  // 13
                            // mb_type 14-15: B_L1_Bi
                            (16, 8, [(false,true),(true,true)]),  // 14
                            (8, 16, [(false,true),(true,true)]),  // 15
                            // mb_type 16-17: B_Bi_L0
                            (16, 8, [(true,true),(true,false)]),  // 16
                            (8, 16, [(true,true),(true,false)]),  // 17
                            // mb_type 18-19: B_Bi_L1
                            (16, 8, [(true,true),(false,true)]),  // 18
                            (8, 16, [(true,true),(false,true)]),  // 19
                            // mb_type 20-21: B_Bi_Bi
                            (16, 8, [(true,true),(true,true)]),   // 20
                            (8, 16, [(true,true),(true,true)]),   // 21
                        ];
                    let entry = B_PART_TABLE[(mb_type - 4) as usize];
                    let (part_w, part_h) = (entry.0, entry.1);
                    let pred_flags = entry.2;

                    // Parse ref_idx: for each list, for each partition (spec 7.3.5.1)
                    let mut part_ref_l0 = [-1i8; 2];
                    let mut part_ref_l1 = [-1i8; 2];
                    for p in 0..2 {
                        if pred_flags[p].0 && num_ref_l0 > 1 {
                            part_ref_l0[p] = reader.read_te(num_ref_l0 - 1)? as i8;
                        } else if pred_flags[p].0 {
                            part_ref_l0[p] = 0;
                        }
                    }
                    for p in 0..2 {
                        if pred_flags[p].1 && num_ref_l1 > 1 {
                            part_ref_l1[p] = reader.read_te(num_ref_l1 - 1)? as i8;
                        } else if pred_flags[p].1 {
                            part_ref_l1[p] = 0;
                        }
                    }

                    // Parse MVD: for each list, for each partition (spec 7.3.5.1)
                    // Order: L0 part0, L0 part1, L1 part0, L1 part1
                    let mut mv_l0_parts = [[0i16; 2]; 2];
                    let mut mv_l1_parts = [[0i16; 2]; 2];
                    for p in 0..2 {
                        if pred_flags[p].0 {
                            // Store ref_idx_l0 before predicting (partition 1 needs partition 0's data)
                            let (py_off, px_off) =
                                if part_h == 8 { (p * 8, 0) } else { (0, p * 8) };
                            for r in (0..part_h).step_by(4) {
                                for c in (0..part_w).step_by(4) {
                                    let lr = (py_off + r) / 4;
                                    let lc = (px_off + c) / 4;
                                    let blk = OFFSET_TO_BLOCK[lr][lc];
                                    self.ref_idx_store_l0[mb_idx * 16 + blk] = part_ref_l0[p];
                                }
                            }
                            let mvd_x = reader.read_se()? as i16;
                            let mvd_y = reader.read_se()? as i16;
                            let (mvp_x, mvp_y) = predict_mv(
                                self.mv_store_l0,
                                self.ref_idx_store_l0,
                                mb_idx,
                                self.mb_width as usize,
                                p,
                                part_w,
                                part_h,
                                part_ref_l0[p],
                                self.mb_slice_id,
                                self.this_slice_id,
                                MbaffCtx {
                                    mbaff: self.mbaff,
                                    mb_field_decoding: self.mb_field_decoding,
                                },
                            );
                            mv_l0_parts[p] = [mvp_x.wrapping_add(mvd_x), mvp_y.wrapping_add(mvd_y)];
                            // Store MV immediately for partition 1 to read partition 0
                            for r in (0..part_h).step_by(4) {
                                for c in (0..part_w).step_by(4) {
                                    let lr = (py_off + r) / 4;
                                    let lc = (px_off + c) / 4;
                                    let blk = OFFSET_TO_BLOCK[lr][lc];
                                    self.mv_store_l0[mb_idx * 16 + blk] = mv_l0_parts[p];
                                }
                            }
                        }
                    }
                    for p in 0..2 {
                        if pred_flags[p].1 {
                            let (py_off, px_off) =
                                if part_h == 8 { (p * 8, 0) } else { (0, p * 8) };
                            for r in (0..part_h).step_by(4) {
                                for c in (0..part_w).step_by(4) {
                                    let lr = (py_off + r) / 4;
                                    let lc = (px_off + c) / 4;
                                    let blk = OFFSET_TO_BLOCK[lr][lc];
                                    self.ref_idx_store_l1[mb_idx * 16 + blk] = part_ref_l1[p];
                                }
                            }
                            let mvd_x = reader.read_se()? as i16;
                            let mvd_y = reader.read_se()? as i16;
                            let (mvp_x, mvp_y) = predict_mv(
                                self.mv_store_l1,
                                self.ref_idx_store_l1,
                                mb_idx,
                                self.mb_width as usize,
                                p,
                                part_w,
                                part_h,
                                part_ref_l1[p],
                                self.mb_slice_id,
                                self.this_slice_id,
                                MbaffCtx {
                                    mbaff: self.mbaff,
                                    mb_field_decoding: self.mb_field_decoding,
                                },
                            );
                            mv_l1_parts[p] = [mvp_x.wrapping_add(mvd_x), mvp_y.wrapping_add(mvd_y)];
                            for r in (0..part_h).step_by(4) {
                                for c in (0..part_w).step_by(4) {
                                    let lr = (py_off + r) / 4;
                                    let lc = (px_off + c) / 4;
                                    let blk = OFFSET_TO_BLOCK[lr][lc];
                                    self.mv_store_l1[mb_idx * 16 + blk] = mv_l1_parts[p];
                                }
                            }
                        }
                    }

                    // Build sub_parts for MC
                    for p in 0..2 {
                        let (py_off, px_off) = if part_h == 8 { (p * 8, 0) } else { (0, p * 8) };
                        sub_parts.push(SubPart {
                            x: px_off,
                            y: py_off,
                            w: part_w,
                            h: part_h,
                            ref_idx_l0: part_ref_l0[p],
                            ref_idx_l1: part_ref_l1[p],
                            mv_l0: mv_l0_parts[p],
                            mv_l1: mv_l1_parts[p],
                            pred_l0: pred_flags[p].0,
                            pred_l1: pred_flags[p].1,
                        });
                    }
                }
                22 => {
                    // B_8x8: 4 sub-MBs, each with its own sub_mb_type
                    // Table 7-17: sub_mb_type 0-12
                    // Format: (sub_w, sub_h, pred_l0, pred_l1)
                    #[rustfmt::skip]
                        const B_SUB_TABLE: [(usize, usize, bool, bool); 13] = [
                            (8, 8, false, false), // 0: B_Direct_8x8
                            (8, 8, true, false),  // 1: B_L0_8x8
                            (8, 8, false, true),  // 2: B_L1_8x8
                            (8, 8, true, true),   // 3: B_Bi_8x8
                            (8, 4, true, false),  // 4: B_L0_8x4
                            (4, 8, true, false),  // 5: B_L0_4x8
                            (8, 4, false, true),  // 6: B_L1_8x4
                            (4, 8, false, true),  // 7: B_L1_4x8
                            (8, 4, true, true),   // 8: B_Bi_8x4
                            (4, 8, true, true),   // 9: B_Bi_4x8
                            (4, 4, true, false),  // 10: B_L0_4x4
                            (4, 4, false, true),  // 11: B_L1_4x4
                            (4, 4, true, true),   // 12: B_Bi_4x4
                        ];

                    let sub_mb_origins = [(0, 0), (0, 8), (8, 0), (8, 8)];
                    let mut sub_mb_types = [0u32; 4];
                    for smt in &mut sub_mb_types {
                        let v = reader.read_ue()?;
                        if v > 12 {
                            return Err(DecodeError::InvalidSyntax("B sub_mb_type out of range"));
                        }
                        *smt = v;
                    }
                    if sub_mb_types
                        .iter()
                        .any(|&smt| smt > 3 || (smt == 0 && !sp.direct_8x8_inference_flag))
                    {
                        no_sub_less_8x8_b = false;
                    }

                    // Parse ref_idx for each 8x8 sub-MB
                    let mut sub_ref_l0 = [-1i8; 4];
                    let mut sub_ref_l1 = [-1i8; 4];
                    for smb in 0..4 {
                        if sub_mb_types[smb] == 0 {
                            continue;
                        } // B_Direct_8x8: no ref_idx
                        let (_, _, pl0, _) = B_SUB_TABLE[sub_mb_types[smb] as usize];
                        if pl0 && num_ref_l0 > 1 {
                            sub_ref_l0[smb] = reader.read_te(num_ref_l0 - 1)? as i8;
                        } else if pl0 {
                            sub_ref_l0[smb] = 0;
                        }
                    }
                    for smb in 0..4 {
                        if sub_mb_types[smb] == 0 {
                            continue;
                        }
                        let (_, _, _, pl1) = B_SUB_TABLE[sub_mb_types[smb] as usize];
                        if pl1 && num_ref_l1 > 1 {
                            sub_ref_l1[smb] = reader.read_te(num_ref_l1 - 1)? as i8;
                        } else if pl1 {
                            sub_ref_l1[smb] = 0;
                        }
                    }

                    // Store ref_idx into cache for MV prediction (both lists)
                    for list in 0..2 {
                        let ref_store = if list == 0 {
                            &mut self.ref_idx_store_l0
                        } else {
                            &mut self.ref_idx_store_l1
                        };
                        let sub_ref = if list == 0 { &sub_ref_l0 } else { &sub_ref_l1 };
                        for smb in 0..4 {
                            let (sy, sx) = sub_mb_origins[smb];
                            for r in (0..8).step_by(4) {
                                for c in (0..8).step_by(4) {
                                    let lr = (sy + r) / 4;
                                    let lc = (sx + c) / 4;
                                    let blk = OFFSET_TO_BLOCK[lr][lc];
                                    ref_store[mb_idx * 16 + blk] = sub_ref[smb];
                                }
                            }
                        }
                    }

                    // Collect sub-partition layouts for each sub-MB
                    struct SubLayout {
                        smb: usize,
                        sx: usize,
                        sy: usize,
                        sub_w: usize,
                        sub_h: usize,
                        pl0: bool,
                        pl1: bool,
                        offsets: Vec<(usize, usize)>, // (dx, dy) within 8x8
                    }
                    let mut layouts: Vec<SubLayout> = Vec::new();
                    for smb in 0..4 {
                        let (sy, sx) = sub_mb_origins[smb];
                        let smt = sub_mb_types[smb] as usize;
                        if smt == 0 {
                            // B_Direct_8x8: handled separately below
                            layouts.push(SubLayout {
                                smb,
                                sx,
                                sy,
                                sub_w: 8,
                                sub_h: 8,
                                pl0: false,
                                pl1: false, // direct mode flag
                                offsets: vec![(0, 0)],
                            });
                            continue;
                        }
                        let (sub_w, sub_h, pl0, pl1) = B_SUB_TABLE[smt];
                        let offsets = match (sub_w, sub_h) {
                            (8, 8) => vec![(0, 0)],
                            (8, 4) => vec![(0, 0), (0, 4)],
                            (4, 8) => vec![(0, 0), (4, 0)],
                            (4, 4) => vec![(0, 0), (4, 0), (0, 4), (4, 4)],
                            _ => unreachable!(),
                        };
                        layouts.push(SubLayout {
                            smb,
                            sx,
                            sy,
                            sub_w,
                            sub_h,
                            pl0,
                            pl1,
                            offsets,
                        });
                    }

                    // Derive B_Direct_8x8 MVs per 4x4 block BEFORE MVD parsing
                    for layout in &layouts {
                        if sub_mb_types[layout.smb] == 0 {
                            self.derive_direct_mvs(mb_idx, layout.smb * 4, 4, sp);
                        }
                    }

                    // Parse MVDs: for each list, for each sub-MB, for each sub-partition
                    // (spec 7.3.5.1 parsing order)
                    struct SubMv {
                        mv_l0: [i16; 2],
                        mv_l1: [i16; 2],
                    }
                    let mut sub_mvs: Vec<SubMv> = Vec::new();
                    // Initialize with zeros
                    for layout in &layouts {
                        for &(_dx, _dy) in &layout.offsets {
                            sub_mvs.push(SubMv {
                                mv_l0: [0; 2],
                                mv_l1: [0; 2],
                            });
                        }
                    }

                    // L0 MVDs first
                    let mut idx = 0;
                    for layout in &layouts {
                        if sub_mb_types[layout.smb] == 0 {
                            idx += 1;
                            continue;
                        } // direct
                        for &(dx, dy) in &layout.offsets {
                            if layout.pl0 {
                                let px = layout.sx + dx;
                                let py = layout.sy + dy;
                                let mvd_x = reader.read_se()? as i16;
                                let mvd_y = reader.read_se()? as i16;
                                let (mvp_x, mvp_y) = predict_mv_sub(
                                    self.mv_store_l0,
                                    self.ref_idx_store_l0,
                                    mb_idx,
                                    self.mb_width as usize,
                                    px,
                                    py,
                                    layout.sub_w,
                                    layout.sub_h,
                                    sub_ref_l0[layout.smb],
                                    self.mb_slice_id,
                                    self.this_slice_id,
                                    MbaffCtx {
                                        mbaff: self.mbaff,
                                        mb_field_decoding: self.mb_field_decoding,
                                    },
                                );
                                let mv = [mvp_x.wrapping_add(mvd_x), mvp_y.wrapping_add(mvd_y)];
                                sub_mvs[idx].mv_l0 = mv;
                                // Store immediately for subsequent sub-partition prediction
                                for r in (0..layout.sub_h).step_by(4) {
                                    for c in (0..layout.sub_w).step_by(4) {
                                        let lr = (py + r) / 4;
                                        let lc = (px + c) / 4;
                                        let blk = OFFSET_TO_BLOCK[lr][lc];
                                        self.mv_store_l0[mb_idx * 16 + blk] = mv;
                                    }
                                }
                            }
                            idx += 1;
                        }
                    }

                    // L1 MVDs second
                    idx = 0;
                    for layout in &layouts {
                        if sub_mb_types[layout.smb] == 0 {
                            idx += 1;
                            continue;
                        }
                        for &(dx, dy) in &layout.offsets {
                            if layout.pl1 {
                                let px = layout.sx + dx;
                                let py = layout.sy + dy;
                                let mvd_x = reader.read_se()? as i16;
                                let mvd_y = reader.read_se()? as i16;
                                let (mvp_x, mvp_y) = predict_mv_sub(
                                    self.mv_store_l1,
                                    self.ref_idx_store_l1,
                                    mb_idx,
                                    self.mb_width as usize,
                                    px,
                                    py,
                                    layout.sub_w,
                                    layout.sub_h,
                                    sub_ref_l1[layout.smb],
                                    self.mb_slice_id,
                                    self.this_slice_id,
                                    MbaffCtx {
                                        mbaff: self.mbaff,
                                        mb_field_decoding: self.mb_field_decoding,
                                    },
                                );
                                let mv = [mvp_x.wrapping_add(mvd_x), mvp_y.wrapping_add(mvd_y)];
                                sub_mvs[idx].mv_l1 = mv;
                                for r in (0..layout.sub_h).step_by(4) {
                                    for c in (0..layout.sub_w).step_by(4) {
                                        let lr = (py + r) / 4;
                                        let lc = (px + c) / 4;
                                        let blk = OFFSET_TO_BLOCK[lr][lc];
                                        self.mv_store_l1[mb_idx * 16 + blk] = mv;
                                    }
                                }
                            }
                            idx += 1;
                        }
                    }

                    // Build sub_parts from stored MVs
                    idx = 0;
                    for layout in &layouts {
                        if sub_mb_types[layout.smb] == 0 {
                            // B_Direct_8x8: coalesce to 8x8 if all 4 blocks share MV/ref
                            let base = mb_idx * 16;
                            let blk0 = OFFSET_TO_BLOCK[layout.sy / 4][layout.sx / 4];
                            let mv0 = self.mv_store_l0[base + blk0];
                            let mv1 = self.mv_store_l1[base + blk0];
                            let r0 = self.ref_idx_store_l0[base + blk0];
                            let r1 = self.ref_idx_store_l1[base + blk0];
                            let uniform = (1..4).all(|sub| {
                                let b = blk0 + sub;
                                self.mv_store_l0[base + b] == mv0
                                    && self.mv_store_l1[base + b] == mv1
                                    && self.ref_idx_store_l0[base + b] == r0
                                    && self.ref_idx_store_l1[base + b] == r1
                            });
                            if uniform {
                                sub_parts.push(SubPart {
                                    x: layout.sx,
                                    y: layout.sy,
                                    w: 8,
                                    h: 8,
                                    ref_idx_l0: r0,
                                    ref_idx_l1: r1,
                                    mv_l0: mv0,
                                    mv_l1: mv1,
                                    pred_l0: r0 >= 0,
                                    pred_l1: r1 >= 0,
                                });
                            } else {
                                for dr in (0..8).step_by(4) {
                                    for dc in (0..8).step_by(4) {
                                        let lr = (layout.sy + dr) / 4;
                                        let lc = (layout.sx + dc) / 4;
                                        let blk = OFFSET_TO_BLOCK[lr][lc];
                                        let mv_l0 = self.mv_store_l0[base + blk];
                                        let mv_l1 = self.mv_store_l1[base + blk];
                                        let ri_l0 = self.ref_idx_store_l0[base + blk];
                                        let ri_l1 = self.ref_idx_store_l1[base + blk];
                                        sub_parts.push(SubPart {
                                            x: layout.sx + dc,
                                            y: layout.sy + dr,
                                            w: 4,
                                            h: 4,
                                            ref_idx_l0: ri_l0,
                                            ref_idx_l1: ri_l1,
                                            mv_l0,
                                            mv_l1,
                                            pred_l0: ri_l0 >= 0,
                                            pred_l1: ri_l1 >= 0,
                                        });
                                    }
                                }
                            }
                            idx += 1;
                        } else {
                            let smt = sub_mb_types[layout.smb] as usize;
                            let (_, _, pl0, pl1) = B_SUB_TABLE[smt];
                            for &(dx, dy) in &layout.offsets {
                                sub_parts.push(SubPart {
                                    x: layout.sx + dx,
                                    y: layout.sy + dy,
                                    w: layout.sub_w,
                                    h: layout.sub_h,
                                    ref_idx_l0: sub_ref_l0[layout.smb],
                                    ref_idx_l1: sub_ref_l1[layout.smb],
                                    mv_l0: sub_mvs[idx].mv_l0,
                                    mv_l1: sub_mvs[idx].mv_l1,
                                    pred_l0: pl0,
                                    pred_l1: pl1,
                                });
                                idx += 1;
                            }
                        }
                    }
                }
                _ => return Err(DecodeError::InvalidSyntax("invalid B-slice mb_type")),
            }

            // Parse CBP using inter table
            let cbp_code = reader.read_ue()? as usize;
            let cbp = crate::residual::map_cbp(cbp_code, false, sp.monochrome)
                .ok_or(DecodeError::from("invalid coded_block_pattern"))?;
            let cbp_luma = cbp & 0x0F;
            let cbp_chroma = cbp >> 4;

            // 8x8 transform flag (High profile, spec 7.3.5)
            let use_8x8_dct_b = sp.transform_8x8_mode_flag
                && cbp_luma != 0
                && no_sub_less_8x8_b
                && reader.read_bit()? != 0;
            // Deblocking skips the internal 4x4 edges of 8x8-transform MBs
            self.mb_is_8x8dct[mb_idx] = use_8x8_dct_b;

            let qp_y = if cbp_luma != 0 || cbp_chroma != 0 {
                let mb_qp_delta = Self::read_mb_qp_delta(reader)?;
                self.next_qp(mb_qp_delta)
            } else {
                self.prev_mb_qp
            };
            self.prev_mb_qp = qp_y;

            // Decode luma residual
            let mut luma_residual = [0i32; 256];
            if use_8x8_dct_b {
                for i8x8 in 0..4 {
                    if cbp_luma & (1 << i8x8) == 0 {
                        for sub in 0..4 {
                            self.nc_luma[mb_idx * 16 + i8x8 * 4 + sub] = 0;
                        }
                        continue;
                    }
                    let mut block_8x8 = [0i32; 64];
                    // CAVLC: decode 4 groups of 16 coefficients (spec 7.3.5.3.2)
                    for i4x4 in 0..4 {
                        let blk = i8x8 * 4 + i4x4;
                        let nc = compute_nc(
                            self.nc_luma,
                            mb_idx,
                            self.mb_width as usize,
                            blk,
                            16,
                            self.mb_slice_id,
                            self.this_slice_id,
                            self.mbaff,
                            self.mb_field_decoding,
                        );
                        let mut quad_coeffs = [0i32; 16];
                        let tc = parse_residual_block_cavlc(reader, &mut quad_coeffs, 16, nc)?;
                        self.nc_luma[mb_idx * 16 + blk] = tc;
                        let scan_base = i4x4 * 16;
                        for k in 0..16 {
                            if quad_coeffs[k] != 0 {
                                block_8x8[zigzag_8x8_cavlc[scan_base + k]] = quad_coeffs[k];
                            }
                        }
                    }
                    dequant_8x8(
                        &mut block_8x8,
                        qp_y + P::QP_BD_OFFSET,
                        &sp.scaling_list_8x8[1],
                    );
                    inverse_dct_8x8(&mut block_8x8);
                    let row_off = (i8x8 / 2) * 8;
                    let col_off = (i8x8 % 2) * 8;
                    for r in 0..8 {
                        for c in 0..8 {
                            luma_residual[(row_off + r) * 16 + col_off + c] = block_8x8[r * 8 + c];
                        }
                    }
                }
            } else {
                for blk in 0..16 {
                    if cbp_luma & (1 << (blk / 4)) != 0 {
                        let nc = compute_nc(
                            self.nc_luma,
                            mb_idx,
                            self.mb_width as usize,
                            blk,
                            16,
                            self.mb_slice_id,
                            self.this_slice_id,
                            self.mbaff,
                            self.mb_field_decoding,
                        );
                        let mut block_coeffs = [0i32; 16];
                        let tc = parse_residual_block_cavlc(reader, &mut block_coeffs, 16, nc)?;
                        self.nc_luma[mb_idx * 16 + blk] = tc;

                        let mut raster = [0i32; 16];
                        for i in 0..16 {
                            let (r, c) = zigzag_4x4[i];
                            raster[r * 4 + c] = block_coeffs[i];
                        }
                        // Use inter scaling list (index 3) for luma
                        dequant_4x4_full(
                            &mut raster,
                            qp_y + P::QP_BD_OFFSET,
                            &sp.scaling_list_4x4[3],
                        );
                        inverse_dct_4x4(&mut raster);

                        let (blk_row, blk_col) = BLOCK_INDEX_TO_OFFSET[blk];
                        for r in 0..4 {
                            for c in 0..4 {
                                luma_residual[(blk_row + r) * 16 + blk_col + c] = raster[r * 4 + c];
                            }
                        }
                    }
                }
            }

            // Luma MC + residual for each sub-partition
            for sub_part in &sub_parts {
                let mut luma_pred = [P::default(); 256];

                if sub_part.pred_l0 && sub_part.pred_l1 {
                    // Bi-prediction: average L0 and L1
                    let mut pred_l0 = vec![P::default(); sub_part.w * sub_part.h];
                    let mut pred_l1 = vec![P::default(); sub_part.w * sub_part.h];
                    let ref_l0 = ref_pic_safe(sp.ref_pic_list_l0, fri(sub_part.ref_idx_l0))
                        .ok_or(DecodeError::InvalidSyntax("empty ref list"))?;
                    let (
                        mc_y_l0,
                        ref_stride_l0,
                        ref_y_off_l0,
                        _mc_cy_l0,
                        _c_ref_stride_l0,
                        _c_ref_off_l0,
                    ) = self.mc_params(mb_idx, mb_y, ref_l0.width as usize, sub_part.ref_idx_l0);
                    P::luma_mc(
                        ref_l0,
                        (mb_x + sub_part.x) as i32,
                        mc_y_l0 + sub_part.y as i32,
                        sub_part.mv_l0[0] as i32,
                        sub_part.mv_l0[1] as i32,
                        sub_part.w,
                        sub_part.h,
                        &mut pred_l0,
                        ref_stride_l0,
                        ref_y_off_l0,
                    );
                    let ref_l1 = ref_pic_safe(sp.ref_pic_list_l1, fri(sub_part.ref_idx_l1))
                        .ok_or(DecodeError::InvalidSyntax("empty ref list"))?;
                    let (
                        mc_y_l1,
                        ref_stride_l1,
                        ref_y_off_l1,
                        _mc_cy_l1,
                        _c_ref_stride_l1,
                        _c_ref_off_l1,
                    ) = self.mc_params(mb_idx, mb_y, ref_l1.width as usize, sub_part.ref_idx_l1);
                    P::luma_mc(
                        ref_l1,
                        (mb_x + sub_part.x) as i32,
                        mc_y_l1 + sub_part.y as i32,
                        sub_part.mv_l1[0] as i32,
                        sub_part.mv_l1[1] as i32,
                        sub_part.w,
                        sub_part.h,
                        &mut pred_l1,
                        ref_stride_l1,
                        ref_y_off_l1,
                    );
                    sp.wctx
                        .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx)
                        .apply_bi(
                            &pred_l0,
                            &pred_l1,
                            &mut luma_pred,
                            sub_part.ref_idx_l0 as usize,
                            sub_part.ref_idx_l1 as usize,
                            false,
                            0,
                        );
                } else if sub_part.pred_l0 {
                    let ref_pic = ref_pic_safe(sp.ref_pic_list_l0, fri(sub_part.ref_idx_l0))
                        .ok_or(DecodeError::InvalidSyntax("empty ref list"))?;
                    let (mc_y, ref_stride, ref_y_off, _mc_cy, _c_ref_stride, _c_ref_off) =
                        self.mc_params(mb_idx, mb_y, ref_pic.width as usize, sub_part.ref_idx_l0);
                    P::luma_mc(
                        ref_pic,
                        (mb_x + sub_part.x) as i32,
                        mc_y + sub_part.y as i32,
                        sub_part.mv_l0[0] as i32,
                        sub_part.mv_l0[1] as i32,
                        sub_part.w,
                        sub_part.h,
                        &mut luma_pred,
                        ref_stride,
                        ref_y_off,
                    );
                    if sp.use_weight == 1 {
                        sp.wctx
                            .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx)
                            .apply_uni(&mut luma_pred, 0, sub_part.ref_idx_l0 as usize, false, 0);
                    }
                } else if sub_part.pred_l1 {
                    let ref_pic = ref_pic_safe(sp.ref_pic_list_l1, fri(sub_part.ref_idx_l1))
                        .ok_or(DecodeError::InvalidSyntax("empty ref list"))?;
                    let (mc_y, ref_stride, ref_y_off, _mc_cy, _c_ref_stride, _c_ref_off) =
                        self.mc_params(mb_idx, mb_y, ref_pic.width as usize, sub_part.ref_idx_l1);
                    P::luma_mc(
                        ref_pic,
                        (mb_x + sub_part.x) as i32,
                        mc_y + sub_part.y as i32,
                        sub_part.mv_l1[0] as i32,
                        sub_part.mv_l1[1] as i32,
                        sub_part.w,
                        sub_part.h,
                        &mut luma_pred,
                        ref_stride,
                        ref_y_off,
                    );
                    if sp.use_weight == 1 {
                        sp.wctx
                            .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx)
                            .apply_uni(&mut luma_pred, 1, sub_part.ref_idx_l1 as usize, false, 0);
                    }
                }

                crate::residual::put_pred_residual(
                    &mut self.frame.y,
                    self.ly_offset + sub_part.y * self.ly_stride + mb_x + sub_part.x,
                    self.ly_stride,
                    &luma_pred,
                    sub_part.w,
                    &luma_residual[sub_part.y * 16 + sub_part.x..],
                    16,
                    (sub_part.w, sub_part.h),
                );
            }

            // Chroma
            for sub_part in sub_parts.iter() {
                let m0 = if sub_part.pred_l0 {
                    let pic = ref_pic_safe(sp.ref_pic_list_l0, fri(sub_part.ref_idx_l0))
                        .ok_or(DecodeError::InvalidSyntax("empty ref list"))?;
                    Some((&**pic, sub_part.ref_idx_l0, sub_part.mv_l0))
                } else {
                    None
                };
                let m1 = if sub_part.pred_l1 {
                    let pic = ref_pic_safe(sp.ref_pic_list_l1, fri(sub_part.ref_idx_l1))
                        .ok_or(DecodeError::InvalidSyntax("empty ref list"))?;
                    Some((&**pic, sub_part.ref_idx_l1, sub_part.mv_l1))
                } else {
                    None
                };
                self.predict_chroma_part(
                    mb_idx,
                    mb_x,
                    mb_y,
                    sp,
                    (sub_part.x, sub_part.y, sub_part.w, sub_part.h),
                    m0,
                    m1,
                );
            }
            self.decode_chroma_residual_cavlc(reader, mb_idx, mb_x, cbp_chroma, qp_y, false, sp)?;

            self.mb_info[mb_idx] = MbInfo {
                mb_type: MbType::Inter,
                qp_y,
                ..Default::default()
            };
            // mb_idx handled by caller
            return Ok(());
        } else if is_inter {
            // === Inter (P) macroblock ===
            let is_p8x8 = mb_type == 3 || mb_type == 4;

            // For P_8x8/P_8x8ref0: parse sub_mb_type for each 8x8 sub-MB
            // sub_mb_type: 0=8x8, 1=8x4, 2=4x8, 3=4x4
            let mut sub_mb_types = [0u32; 4];
            if is_p8x8 {
                for smt in &mut sub_mb_types {
                    *smt = reader.read_ue()?;
                }
            }

            // Collect all sub-partition info: (x_off, y_off, self.width, self.height, ref_idx)
            // For non-P_8x8: 1-2 partitions as before
            // For P_8x8: up to 16 sub-partitions across 4 sub-MBs
            #[derive(Clone, Copy, Default)]
            struct SubPart {
                x: usize,
                y: usize,
                w: usize,
                h: usize,
                ref_idx: i8,
                mv: [i16; 2],
            }
            let mut sub_parts = [SubPart::default(); 16];
            let mut sub_part_count = 0usize;

            if is_p8x8 {
                // 8x8 sub-MB origins within the macroblock
                let sub_mb_origins = [(0, 0), (0, 8), (8, 0), (8, 8)];

                // Parse ref_idx for each 8x8 sub-MB
                // Field-coded MBs double the effective ref count (each frame ref → 2 field refs)
                let eff_num_ref_l0 = self.effective_num_ref(mb_idx, sp.num_ref_idx_l0_active);
                let mut sub_ref = [0i8; 4];
                if mb_type == 3 {
                    // P_8x8: parse ref_idx per sub-MB
                    for sr in &mut sub_ref {
                        if eff_num_ref_l0 > 1 {
                            *sr = reader.read_te(eff_num_ref_l0 - 1)? as i8;
                        }
                    }
                }
                // P_8x8ref0 (mb_type=4): all ref_idx = 0 (already initialized)
                // Parse MVD for each sub-partition and store MVs
                for smb in 0..4 {
                    let (sy, sx) = sub_mb_origins[smb];
                    let ref_idx = sub_ref[smb];

                    let sub_parts_layout = match sub_mb_types[smb] {
                        0 => [(0, 0, 8, 8), (0, 0, 0, 0), (0, 0, 0, 0), (0, 0, 0, 0)],
                        1 => [(0, 0, 8, 4), (0, 4, 8, 4), (0, 0, 0, 0), (0, 0, 0, 0)],
                        2 => [(0, 0, 4, 8), (4, 0, 4, 8), (0, 0, 0, 0), (0, 0, 0, 0)],
                        3 => [(0, 0, 4, 4), (4, 0, 4, 4), (0, 4, 4, 4), (4, 4, 4, 4)],
                        _ => return Err(DecodeError::from("invalid sub_mb_type")),
                    };
                    let layout_count = match sub_mb_types[smb] {
                        0 => 1,
                        1 | 2 => 2,
                        3 => 4,
                        _ => unreachable!(),
                    };

                    for &(dx, dy, spw, sph) in sub_parts_layout.iter().take(layout_count) {
                        let px = sx + dx;
                        let py = sy + dy;
                        let mvd_x = reader.read_se()? as i16;
                        let mvd_y = reader.read_se()? as i16;
                        let (mvp_x, mvp_y) = predict_mv_sub(
                            self.mv_store_l0,
                            self.ref_idx_store_l0,
                            mb_idx,
                            self.mb_width as usize,
                            px,
                            py,
                            spw,
                            sph,
                            ref_idx,
                            self.mb_slice_id,
                            self.this_slice_id,
                            MbaffCtx {
                                mbaff: self.mbaff,
                                mb_field_decoding: self.mb_field_decoding,
                            },
                        );
                        let mv = [mvp_x.wrapping_add(mvd_x), mvp_y.wrapping_add(mvd_y)];
                        // Store MV for all 4x4 blocks in this sub-partition
                        for r in (0..sph).step_by(4) {
                            for c in (0..spw).step_by(4) {
                                let lr = (py + r) / 4;
                                let lc = (px + c) / 4;
                                let blk = OFFSET_TO_BLOCK[lr][lc];
                                self.mv_store_l0[mb_idx * 16 + blk] = mv;
                                self.ref_idx_store_l0[mb_idx * 16 + blk] = ref_idx;
                            }
                        }

                        sub_parts[sub_part_count] = SubPart {
                            x: px,
                            y: py,
                            w: spw,
                            h: sph,
                            ref_idx,
                            mv,
                        };
                        sub_part_count += 1;
                    }
                }
            } else {
                // P_L0_16x16, P16x8, P8x16
                let (part_w, part_h, num_parts) = match mb_type {
                    0 => (16usize, 16usize, 1usize),
                    1 => (16, 8, 2),
                    2 => (8, 16, 2),
                    _ => unreachable!(),
                };

                let eff_num_ref_l0 = self.effective_num_ref(mb_idx, sp.num_ref_idx_l0_active);
                let mut part_ref = [0i8; 2];
                for ref_entry in part_ref.iter_mut().take(num_parts) {
                    if eff_num_ref_l0 > 1 {
                        *ref_entry = reader.read_te(eff_num_ref_l0 - 1)? as i8;
                    }
                }

                #[allow(clippy::needless_range_loop)]
                for p in 0..num_parts {
                    let mvd_x = reader.read_se()? as i16;
                    let mvd_y = reader.read_se()? as i16;
                    let (mvp_x, mvp_y) = predict_mv(
                        self.mv_store_l0,
                        self.ref_idx_store_l0,
                        mb_idx,
                        self.mb_width as usize,
                        p,
                        part_w,
                        part_h,
                        part_ref[p],
                        self.mb_slice_id,
                        self.this_slice_id,
                        MbaffCtx {
                            mbaff: self.mbaff,
                            mb_field_decoding: self.mb_field_decoding,
                        },
                    );
                    let mv = [mvp_x.wrapping_add(mvd_x), mvp_y.wrapping_add(mvd_y)];
                    let (py_off, px_off) = match mb_type {
                        1 => (p * 8, 0),
                        2 => (0, p * 8),
                        _ => (0, 0),
                    };

                    // Store MV/ref immediately
                    for r in (0..part_h).step_by(4) {
                        for c in (0..part_w).step_by(4) {
                            let lr = (py_off + r) / 4;
                            let lc = (px_off + c) / 4;
                            let blk = OFFSET_TO_BLOCK[lr][lc];
                            self.mv_store_l0[mb_idx * 16 + blk] = mv;
                            self.ref_idx_store_l0[mb_idx * 16 + blk] = part_ref[p];
                        }
                    }

                    sub_parts[sub_part_count] = SubPart {
                        x: px_off,
                        y: py_off,
                        w: part_w,
                        h: part_h,
                        ref_idx: part_ref[p],
                        mv,
                    };
                    sub_part_count += 1;
                }
            }

            // Parse CBP using inter table
            let cbp_code = reader.read_ue()? as usize;
            let cbp = crate::residual::map_cbp(cbp_code, false, sp.monochrome)
                .ok_or(DecodeError::from("invalid coded_block_pattern"))?;
            let cbp_luma = cbp & 0x0F;
            let cbp_chroma = cbp >> 4;
            // 8x8 transform flag (High profile inter MBs, spec 7.3.5).
            // Only present when noSubMbPartSizeLessThan8x8Flag is true.
            let no_sub_less_8x8 = if is_p8x8 {
                sub_mb_types.iter().all(|&smt| smt == 0) // P_8x8: smt=0 means 8x8 sub-partition
            } else {
                true // P_16x16, P_16x8, P_8x16 always >= 8x8
            };
            let use_8x8_dct = sp.transform_8x8_mode_flag
                && cbp_luma != 0
                && no_sub_less_8x8
                && reader.read_bit()? != 0;
            // Deblocking skips the internal 4x4 edges of 8x8-transform MBs
            self.mb_is_8x8dct[mb_idx] = use_8x8_dct;

            let qp_y = if cbp_luma != 0 || cbp_chroma != 0 {
                let mb_qp_delta = Self::read_mb_qp_delta(reader)?;
                self.next_qp(mb_qp_delta)
            } else {
                self.prev_mb_qp
            };
            self.prev_mb_qp = qp_y;
            // Decode luma residual
            let mut luma_residual = [0i32; 256];
            if use_8x8_dct {
                // 8x8 transform: 4 blocks of 64 coefficients each
                let scale_8x8 = if is_inter {
                    &sp.scaling_list_8x8[1]
                } else {
                    &sp.scaling_list_8x8[0]
                };
                for i8x8 in 0..4 {
                    if cbp_luma & (1 << i8x8) == 0 {
                        continue;
                    }
                    let mut block_8x8 = [0i32; 64];
                    // Decode 4 groups of 16 coefficients via CAVLC
                    for i4x4 in 0..4 {
                        let blk = i8x8 * 4 + i4x4;
                        let nc = compute_nc(
                            self.nc_luma,
                            mb_idx,
                            self.mb_width as usize,
                            blk,
                            16,
                            self.mb_slice_id,
                            self.this_slice_id,
                            self.mbaff,
                            self.mb_field_decoding,
                        );
                        let mut quad_coeffs = [0i32; 16];
                        let tc = parse_residual_block_cavlc(reader, &mut quad_coeffs, 16, nc)?;
                        self.nc_luma[mb_idx * 16 + blk] = tc;
                        // Place into 8x8 block using scan table
                        let scan_base = i4x4 * 16;
                        for k in 0..16 {
                            if quad_coeffs[k] != 0 {
                                block_8x8[zigzag_8x8_cavlc[scan_base + k]] = quad_coeffs[k];
                            }
                        }
                    }
                    dequant_8x8(&mut block_8x8, qp_y + P::QP_BD_OFFSET, scale_8x8);
                    inverse_dct_8x8(&mut block_8x8);
                    // Copy 8x8 residual into 16x16 luma residual
                    let row_off = (i8x8 / 2) * 8;
                    let col_off = (i8x8 % 2) * 8;
                    for r in 0..8 {
                        for c in 0..8 {
                            luma_residual[(row_off + r) * 16 + col_off + c] = block_8x8[r * 8 + c];
                        }
                    }
                }
            } else {
                // 4x4 transform (existing path)
                for blk in 0..16 {
                    if cbp_luma & (1 << (blk / 4)) != 0 {
                        let nc = compute_nc(
                            self.nc_luma,
                            mb_idx,
                            self.mb_width as usize,
                            blk,
                            16,
                            self.mb_slice_id,
                            self.this_slice_id,
                            self.mbaff,
                            self.mb_field_decoding,
                        );
                        let mut block_coeffs = [0i32; 16];
                        let tc = parse_residual_block_cavlc(reader, &mut block_coeffs, 16, nc)?;
                        self.nc_luma[mb_idx * 16 + blk] = tc;

                        let mut raster = [0i32; 16];
                        for i in 0..16 {
                            let (r, c) = zigzag_4x4[i];
                            raster[r * 4 + c] = block_coeffs[i];
                        }
                        dequant_4x4_full(
                            &mut raster,
                            qp_y + P::QP_BD_OFFSET,
                            &sp.scaling_list_4x4[3],
                        );
                        inverse_dct_4x4(&mut raster);

                        let (blk_row, blk_col) = BLOCK_INDEX_TO_OFFSET[blk];
                        for r in 0..4 {
                            for c in 0..4 {
                                luma_residual[(blk_row + r) * 16 + blk_col + c] = raster[r * 4 + c];
                            }
                        }
                    }
                }
            } // close if use_8x8_dct else

            // Motion compensate and add residual for each sub-partition
            for sub_part in sub_parts.iter().take(sub_part_count) {
                let fri = self.frame_ref_idx(mb_idx, sub_part.ref_idx);
                let ref_pic = sp
                    .ref_pic_list
                    .get(fri as usize)
                    .or_else(|| sp.ref_pic_list.last())
                    .ok_or(DecodeError::InvalidSyntax(
                        "P sub-partition references empty ref_pic_list",
                    ))?;
                let (mc_y, ref_stride, ref_y_off, _mc_cy, _c_ref_stride, _c_ref_off) =
                    self.mc_params(mb_idx, mb_y, ref_pic.width as usize, sub_part.ref_idx);
                let mut luma_pred = [P::default(); 256];
                P::luma_mc(
                    ref_pic,
                    (mb_x + sub_part.x) as i32,
                    mc_y + sub_part.y as i32,
                    sub_part.mv[0] as i32,
                    sub_part.mv[1] as i32,
                    sub_part.w,
                    sub_part.h,
                    &mut luma_pred,
                    ref_stride,
                    ref_y_off,
                );
                if sp.use_weight == 1 {
                    sp.wctx
                        .mb(self.mbaff && self.mb_field_decoding[mb_idx / 2], mb_idx)
                        .apply_uni(&mut luma_pred, 0, sub_part.ref_idx as usize, false, 0);
                }
                crate::residual::put_pred_residual(
                    &mut self.frame.y,
                    self.ly_offset + sub_part.y * self.ly_stride + mb_x + sub_part.x,
                    self.ly_stride,
                    &luma_pred,
                    sub_part.w,
                    &luma_residual[sub_part.y * 16 + sub_part.x..],
                    16,
                    (sub_part.w, sub_part.h),
                );
            }

            // Chroma (P-slice inter)
            for sub_part in sub_parts.iter().take(sub_part_count) {
                let pic = sp
                    .ref_pic_list
                    .get(self.frame_ref_idx(mb_idx, sub_part.ref_idx) as usize)
                    .or_else(|| sp.ref_pic_list.last())
                    .ok_or(DecodeError::InvalidSyntax(
                        "P sub-partition chroma references empty ref_pic_list",
                    ))?;
                self.predict_chroma_part(
                    mb_idx,
                    mb_x,
                    mb_y,
                    sp,
                    (sub_part.x, sub_part.y, sub_part.w, sub_part.h),
                    Some((&**pic, sub_part.ref_idx, sub_part.mv)),
                    None,
                );
            }
            self.decode_chroma_residual_cavlc(reader, mb_idx, mb_x, cbp_chroma, qp_y, false, sp)?;

            self.mb_info[mb_idx] = MbInfo {
                mb_type: MbType::Inter,
                qp_y,
                ..Default::default()
            };
            // mb_idx handled by caller
            return Ok(());
        }

        // === Intra macroblock (I4x4, I16x16, I_PCM) ===
        // Cross-slice intra prediction: neighbors from other slices unavailable (spec 6.4.1)
        // constrained_intra_pred_flag: inter-predicted neighbors also unavailable
        let above_mb_avail = self
            .above_mb(mb_idx)
            .is_some_and(|above| self.is_intra_neighbor_avail(above, sp));
        let left_halves = self.left_intra_avail(mb_idx, sp);
        let left_mb_avail = left_halves[0] && left_halves[1];
        let above_left_mb_avail = self.intra_corner_avail(mb_idx, -1, -1, sp);
        let above_right_mb_avail = self.intra_corner_avail(mb_idx, 16, -1, sp);

        // Variables shared between I4x4/I16x16 for chroma reconstruction
        let intra_chroma_pred_mode;
        let cbp_chroma: u8;
        let qp_y;

        if mb_type == 0 {
            // === I_NxN (I4x4 or I8x8) macroblock ===

            // Check 8x8 transform flag
            let use_8x8_intra = sp.transform_8x8_mode_flag && reader.read_bit()? != 0;
            // Deblocking skips the internal 4x4 edges of 8x8-transform MBs
            self.mb_is_8x8dct[mb_idx] = use_8x8_intra;

            let intra_avail = self.intra_avail_map(sp);

            // Parse prediction modes: 4 for I8x8, 16 for I4x4
            let num_modes = if use_8x8_intra { 4 } else { 16 };
            let mut pred_modes = [2u8; 16];
            for blk_idx in 0..num_modes {
                let blk = if use_8x8_intra { blk_idx * 4 } else { blk_idx };
                let prev_flag = reader.read_bit()?;
                let predicted = predict_i4x4_mode(
                    self.i4x4_modes,
                    mb_idx,
                    self.mb_width as usize,
                    blk,
                    self.mb_slice_id,
                    self.this_slice_id,
                    &intra_avail,
                    self.mbaff,
                    self.mb_field_decoding,
                );
                let mode = if prev_flag != 0 {
                    predicted
                } else {
                    let rem = reader.read_bits(3)? as u8;
                    if rem < predicted {
                        rem
                    } else {
                        rem + 1
                    }
                };
                if use_8x8_intra {
                    // Store same mode for all 4 sub-blocks
                    for sub in 0..4 {
                        pred_modes[blk_idx * 4 + sub] = mode;
                        self.i4x4_modes[mb_idx * 16 + blk_idx * 4 + sub] = mode;
                    }
                } else {
                    pred_modes[blk] = mode;
                    self.i4x4_modes[mb_idx * 16 + blk] = mode;
                }
            }
            intra_chroma_pred_mode = if sp.monochrome {
                0
            } else {
                reader.read_ue()? as u8
            };

            let cbp_code = reader.read_ue()? as usize;
            let cbp = crate::residual::map_cbp(cbp_code, true, sp.monochrome)
                .ok_or(DecodeError::from("invalid coded_block_pattern"))?;
            let cbp_luma = cbp & 0x0F;
            cbp_chroma = cbp >> 4;

            if cbp_luma != 0 || cbp_chroma != 0 {
                let mb_qp_delta = Self::read_mb_qp_delta(reader)?;
                qp_y = self.next_qp(mb_qp_delta);
            } else {
                qp_y = self.prev_mb_qp;
            }
            self.prev_mb_qp = qp_y;

            // Parse luma residual and reconstruct
            if use_8x8_intra {
                // I8x8: decode 4 8x8 blocks with 8x8 transform
                let mut luma_residual = [0i32; 256];
                for i8x8 in 0..4 {
                    if cbp_luma & (1 << i8x8) == 0 {
                        continue;
                    }
                    let mut block_8x8 = [0i32; 64];
                    for i4x4 in 0..4 {
                        let blk = i8x8 * 4 + i4x4;
                        let nc = compute_nc(
                            self.nc_luma,
                            mb_idx,
                            self.mb_width as usize,
                            blk,
                            16,
                            self.mb_slice_id,
                            self.this_slice_id,
                            self.mbaff,
                            self.mb_field_decoding,
                        );
                        let mut quad_coeffs = [0i32; 16];
                        let tc = parse_residual_block_cavlc(reader, &mut quad_coeffs, 16, nc)?;
                        self.nc_luma[mb_idx * 16 + blk] = tc;
                        let scan_base = i4x4 * 16;
                        for k in 0..16 {
                            if quad_coeffs[k] != 0 {
                                block_8x8[zigzag_8x8_cavlc[scan_base + k]] = quad_coeffs[k];
                            }
                        }
                    }
                    dequant_8x8(
                        &mut block_8x8,
                        qp_y + P::QP_BD_OFFSET,
                        &sp.scaling_list_8x8[0],
                    );
                    inverse_dct_8x8(&mut block_8x8);

                    let row_off = (i8x8 / 2) * 8;
                    let col_off = (i8x8 % 2) * 8;
                    for r in 0..8 {
                        for c in 0..8 {
                            luma_residual[(row_off + r) * 16 + col_off + c] = block_8x8[r * 8 + c];
                        }
                    }
                }
                // I8x8 prediction + residual reconstruction
                for i8x8 in 0..4 {
                    self.reconstruct_luma_8x8_block(
                        i8x8,
                        mb_x,
                        pred_modes[i8x8 * 4],
                        &luma_residual,
                        above_mb_avail,
                        left_halves,
                        above_left_mb_avail,
                        above_right_mb_avail,
                    );
                }
            } else {
                // I4x4: existing 4x4 path
                for blk in 0..16 {
                    let (blk_row, blk_col) = BLOCK_INDEX_TO_OFFSET[blk];
                    let px = mb_x + blk_col;
                    let py = mb_y + blk_row;

                    // Parse residual
                    let mut block_coeffs = [0i32; 16];
                    if cbp_luma & (1 << (blk / 4)) != 0 {
                        let nc = compute_nc(
                            self.nc_luma,
                            mb_idx,
                            self.mb_width as usize,
                            blk,
                            16,
                            self.mb_slice_id,
                            self.this_slice_id,
                            self.mbaff,
                            self.mb_field_decoding,
                        );
                        let tc = parse_residual_block_cavlc(reader, &mut block_coeffs, 16, nc)?;
                        self.nc_luma[mb_idx * 16 + blk] = tc;

                        // Unzigzag: convert from zigzag scan order to raster order
                        let mut raster = [0i32; 16];
                        for i in 0..16 {
                            let (r, c) = zigzag_4x4[i];
                            raster[r * 4 + c] = block_coeffs[i];
                        }
                        block_coeffs = raster;

                        dequant_4x4_full(
                            &mut block_coeffs,
                            qp_y + P::QP_BD_OFFSET,
                            &sp.scaling_list_4x4[0],
                        );
                    }
                    inverse_dct_4x4(&mut block_coeffs);
                    // I4x4 prediction + residual
                    self.reconstruct_luma_4x4_block(
                        px,
                        py,
                        mb_x,
                        mb_y,
                        blk,
                        pred_modes[blk],
                        &block_coeffs,
                        above_mb_avail,
                        left_halves,
                        above_left_mb_avail,
                        above_right_mb_avail,
                    );
                }
            } // close if use_8x8_intra else
        } else if mb_type <= 24 {
            // === I16x16 macroblock ===
            let mt = mb_type - 1;
            let intra16x16_pred_mode = (mt % 4) as u8;
            cbp_chroma = ((mt / 4) % 3) as u8;
            let cbp_luma = if mt >= 12 { 15u8 } else { 0u8 };

            intra_chroma_pred_mode = if sp.monochrome {
                0
            } else {
                reader.read_ue()? as u8
            };

            let mb_qp_delta = Self::read_mb_qp_delta(reader)?;
            qp_y = self.next_qp(mb_qp_delta);
            self.prev_mb_qp = qp_y;

            // Parse luma DC
            let mut luma_dc = [0i32; 16];
            let nc_dc = compute_nc(
                self.nc_luma,
                mb_idx,
                self.mb_width as usize,
                0,
                16,
                self.mb_slice_id,
                self.this_slice_id,
                self.mbaff,
                self.mb_field_decoding,
            );
            parse_residual_block_cavlc(reader, &mut luma_dc, 16, nc_dc)?;

            // Parse luma AC
            let mut luma_ac_scan = [[0i32; 15]; 16];
            if cbp_luma != 0 {
                for blk in 0..16 {
                    let nc = compute_nc(
                        self.nc_luma,
                        mb_idx,
                        self.mb_width as usize,
                        blk,
                        16,
                        self.mb_slice_id,
                        self.this_slice_id,
                        self.mbaff,
                        self.mb_field_decoding,
                    );
                    let tc = parse_residual_block_cavlc(reader, &mut luma_ac_scan[blk], 15, nc)?;
                    self.nc_luma[mb_idx * 16 + blk] = tc;
                }
            }

            // Unzigzag DC, Hadamard, dequant
            let mut luma_dc_raster = [0i32; 16];
            for i in 0..16 {
                let (r, c) = zigzag_4x4[i];
                luma_dc_raster[r * 4 + c] = luma_dc[i];
            }
            inverse_hadamard_4x4(&mut luma_dc_raster);
            dequant_luma_dc_i16x16(
                &mut luma_dc_raster,
                qp_y + P::QP_BD_OFFSET,
                sp.scaling_list_4x4[0][0],
            );

            const DC_RASTER_TO_BLOCK: [usize; 16] =
                [0, 1, 4, 5, 2, 3, 6, 7, 8, 9, 12, 13, 10, 11, 14, 15];

            let mut luma_residual = [0i32; 256];
            for blk in 0..16 {
                let (blk_row, blk_col) = BLOCK_INDEX_TO_OFFSET[blk];
                let mut block_raster = [0i32; 16];
                let dc_idx = DC_RASTER_TO_BLOCK.iter().position(|&b| b == blk).unwrap();
                block_raster[0] = luma_dc_raster[dc_idx];

                if cbp_luma != 0 {
                    for scan_idx in 0..15 {
                        let (r, c) = zigzag_4x4[scan_idx + 1];
                        block_raster[r * 4 + c] = luma_ac_scan[blk][scan_idx];
                    }
                    dequant_4x4_ac_raster(
                        &mut block_raster,
                        qp_y + P::QP_BD_OFFSET,
                        &sp.scaling_list_4x4[0],
                    );
                }

                inverse_dct_4x4(&mut block_raster);

                for r in 0..4 {
                    for c in 0..4 {
                        luma_residual[(blk_row + r) * 16 + blk_col + c] = block_raster[r * 4 + c];
                    }
                }
            }

            // I16x16 prediction + residual
            self.reconstruct_luma_16x16(
                mb_x,
                intra16x16_pred_mode,
                &luma_residual,
                above_mb_avail,
                left_mb_avail,
                above_left_mb_avail,
            );
        } else if mb_type == 25 {
            // === I_PCM macroblock ===
            reader.align_to_byte();
            self.write_pcm_samples(
                || reader.read_bits(P::BIT_DEPTH as u8).ok().map(|v| v as i32),
                mb_x,
                sp.monochrome,
            )?;
            for blk in 0..16 {
                self.nc_luma[mb_idx * 16 + blk] = 16;
            }
            for blk in 0..8 {
                self.nc_cb[mb_idx * 8 + blk] = 16;
                self.nc_cr[mb_idx * 8 + blk] = 16;
            }
            self.mb_info[mb_idx] = MbInfo {
                mb_type: MbType::Ipcm,
                qp_y: 0,
                ..Default::default()
            };
            // mb_qp_delta is absent: QP_Y (and the next MB's prediction) is
            // unchanged; deblocking uses QP 0 for I_PCM (mb_info.qp_y)
            return Ok(());
        } else {
            return Err(DecodeError::from("unsupported mb_type for I slice"));
        }

        self.mb_info[mb_idx] = MbInfo {
            mb_type: MbType::Intra,
            qp_y,
            ..Default::default()
        };

        // Intra MBs keep ref_idx=-1 (default) and mv=(0,0) (default).
        // The -1 ref_idx ensures predict_mv's match_count logic correctly
        // excludes intra neighbors from directional prediction.

        // === Reconstruct chroma (shared by I4x4 and I16x16) ===
        self.write_chroma_intra_pred(
            mb_x,
            intra_chroma_pred_mode,
            above_mb_avail,
            left_halves,
            above_left_mb_avail,
        );
        self.decode_chroma_residual_cavlc(reader, mb_idx, mb_x, cbp_chroma, qp_y, true, sp)?;
        Ok(())
    }
}
