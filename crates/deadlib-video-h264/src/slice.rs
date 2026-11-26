use alloc::vec::Vec;

use crate::bitstream::BitstreamReader;
use crate::nal::NalUnitType;
use crate::pps::Pps;
use crate::sps::Sps;

/// Slice types per H.264 Table 7-6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceType {
    P,
    B,
    I,
    Sp,
    Si,
}

impl SliceType {
    pub fn from_raw(val: u32) -> Result<Self, &'static str> {
        match val {
            0 | 5 => Ok(SliceType::P),
            1 | 6 => Ok(SliceType::B),
            2 | 7 => Ok(SliceType::I),
            3 | 8 => Ok(SliceType::Sp),
            4 | 9 => Ok(SliceType::Si),
            _ => Err("invalid slice_type"),
        }
    }
}

/// Slice header (H.264 spec section 7.3.3).
#[derive(Debug)]
pub struct SliceHeader {
    pub first_mb_in_slice: u32,
    pub slice_type: SliceType,
    pub pic_parameter_set_id: u32,
    pub frame_num: u32,
    pub idr_pic_id: Option<u32>,
    /// 0 for a primary coded picture; above 0 for a redundant one, which the
    /// decoder ignores (spec 7.4.3, as FFmpeg does).
    pub redundant_pic_cnt: u32,
    pub no_output_of_prior_pics_flag: bool,
    pub long_term_reference_flag: bool,
    pub slice_qp_delta: i32,
    pub disable_deblocking_filter_idc: u32,
    pub slice_alpha_c0_offset_div2: i32,
    pub slice_beta_offset_div2: i32,
    // POC fields (stored for DPB ordering)
    pub pic_order_cnt_lsb: u32,
    pub delta_pic_order_cnt_bottom: i32,
    pub delta_pic_order_cnt: [i32; 2],
    /// Number of active L0 reference frames for P/B slices (1-based).
    pub num_ref_idx_l0_active: u32,
    /// Number of active L1 reference frames for B slices (1-based).
    pub num_ref_idx_l1_active: u32,
    /// Spatial (true) vs temporal (false) direct mode for B slices.
    pub direct_spatial_mv_pred_flag: bool,
    /// MMCO operations: (op_code, parameter). Currently only op=1 (mark short-term unused).
    pub mmco_ops: Vec<(u32, u32)>,
    /// adaptive_ref_pic_marking_mode_flag: marking by `mmco_ops` instead of
    /// the sliding window (spec 8.2.5.3)
    pub adaptive_ref_pic_marking: bool,
    /// Ref pic list modification ops for L0: (idc, abs_diff_pic_num_minus1).
    pub ref_list_mod_l0: Vec<(u32, u32)>,
    /// Ref pic list modification ops for L1.
    pub ref_list_mod_l1: Vec<(u32, u32)>,
    /// CABAC context initialization index (0-2) for P/B slices. Only valid when entropy_coding_mode_flag=1.
    pub cabac_init_idc: u32,
    /// Weighted prediction table (spec 7.3.3.2).
    pub weight_table: Option<PredWeightTable>,
    /// True if current picture is a field picture (only when !frame_mbs_only_flag).
    pub field_pic_flag: bool,
    /// True if current field is the bottom field (only when field_pic_flag).
    pub bottom_field_flag: bool,
    /// MbaffFrameFlag = mb_adaptive_frame_field_flag && !field_pic_flag
    pub mbaff_frame_flag: bool,
}

/// Weighted prediction parameters from pred_weight_table().
/// Stores per-reference weight and offset for luma and chroma.
#[derive(Debug, Clone)]
pub struct PredWeightTable {
    pub luma_log2_weight_denom: u32,
    pub chroma_log2_weight_denom: u32,
    /// Per-reference weights for L0.
    pub l0: Vec<RefWeight>,
    /// L1 weights (B-slices only).
    pub l1: Vec<RefWeight>,
}

#[derive(Debug, Clone)]
pub struct RefWeight {
    pub luma_weight: i32,
    pub luma_offset: i32,
    pub chroma_weight: [i32; 2], // Cb, Cr
    pub chroma_offset: [i32; 2], // Cb, Cr
}

impl SliceHeader {
    pub fn qp_y(&self, pps: &Pps) -> i32 {
        26 + pps.pic_init_qp_minus26 + self.slice_qp_delta
    }
}

/// Bits of an MMCO 3 parameter holding difference_of_pic_nums_minus1; the
/// long_term_frame_idx sits above them.
pub(crate) const MMCO_DIFF_BITS: u32 = 17;

/// difference_of_pic_nums_minus1 (MMCO 1 and 3): below MaxPicNum, at most
/// 2^17 for field pictures.
fn read_pic_num_diff(r: &mut BitstreamReader) -> Result<u32, &'static str> {
    match r.read_ue()? {
        v if v < 1 << MMCO_DIFF_BITS => Ok(v),
        _ => Err("difference_of_pic_nums_minus1 out of range"),
    }
}

/// long_term_pic_num or long_term_frame_idx (MMCO 2, 3 and 6): below 32.
fn read_long_term_idx(r: &mut BitstreamReader) -> Result<u32, &'static str> {
    match r.read_ue()? {
        v if v < 32 => Ok(v),
        _ => Err("long-term reference index out of range"),
    }
}

/// Parse a slice header from RBSP data. Returns the header and a reader
/// positioned at the start of slice data (macroblock layer).
pub fn parse_slice_header(
    rbsp: &[u8],
    sps: &Sps,
    pps: &Pps,
    nal_unit_type: NalUnitType,
    nal_ref_idc: u8,
) -> Result<(SliceHeader, BitstreamReader), &'static str> {
    let mut r = BitstreamReader::new(rbsp);

    let first_mb_in_slice = r.read_ue()?;
    let slice_type_raw = r.read_ue()?;
    let slice_type = SliceType::from_raw(slice_type_raw)?;
    // SP/SI slices are not decoded; reject them before the header fields
    // only they have (sp_for_switch_flag, slice_qs_delta)
    if matches!(slice_type, SliceType::Sp | SliceType::Si) {
        return Err("unsupported slice type (SP/SI)");
    }
    let pic_parameter_set_id = r.read_ue()?;

    let frame_num_bits = sps.log2_max_frame_num_minus4 + 4;
    let frame_num = r.read_bits(frame_num_bits as u8)?;
    // field_pic_flag / bottom_field_flag (spec 7.3.3)
    let mut field_pic_flag = false;
    let mut bottom_field_flag = false;
    if !sps.frame_mbs_only_flag {
        field_pic_flag = r.read_bit()? != 0;
        if field_pic_flag {
            bottom_field_flag = r.read_bit()? != 0;
        }
    }
    let mbaff_frame_flag = sps.mb_adaptive_frame_field_flag && !field_pic_flag;
    let mut idr_pic_id = None;
    if nal_unit_type == NalUnitType::SliceIdr {
        idr_pic_id = Some(r.read_ue()?);
    }

    // pic_order_cnt_type == 0: read pic_order_cnt_lsb
    // pic_order_cnt_type == 1: read delta_pic_order_cnt
    // pic_order_cnt_type == 2: nothing
    let mut pic_order_cnt_lsb = 0u32;
    let mut delta_pic_order_cnt_bottom = 0i32;
    let mut delta_pic_order_cnt = [0i32; 2];

    if sps.pic_order_cnt_type == 0 {
        let poc_lsb_bits = sps.log2_max_pic_order_cnt_lsb_minus4 + 4;
        pic_order_cnt_lsb = r.read_bits(poc_lsb_bits as u8)?;
        if pps.bottom_field_pic_order_in_frame_present_flag && !field_pic_flag {
            delta_pic_order_cnt_bottom = r.read_se()?;
        }
    } else if sps.pic_order_cnt_type == 1 && !sps.delta_pic_order_always_zero_flag {
        delta_pic_order_cnt[0] = r.read_se()?;
        if pps.bottom_field_pic_order_in_frame_present_flag && !field_pic_flag {
            delta_pic_order_cnt[1] = r.read_se()?;
        }
    }
    let redundant_pic_cnt = if pps.redundant_pic_cnt_present_flag {
        match r.read_ue()? {
            v @ 0..=127 => v,
            _ => return Err("redundant_pic_cnt out of range"),
        }
    } else {
        0
    };
    // direct_spatial_mv_pred_flag (B-slices only, spec 7.3.3)
    let mut direct_spatial_mv_pred_flag = false;
    if slice_type == SliceType::B {
        direct_spatial_mv_pred_flag = r.read_bit()? != 0;
    }

    // num_ref_idx_active_override (for P/B slices)
    let mut num_ref_idx_l0_active = if slice_type == SliceType::I || slice_type == SliceType::Si {
        0
    } else {
        pps.num_ref_idx_l0_default_active_minus1 + 1
    };
    let mut num_ref_idx_l1_active = if slice_type == SliceType::B {
        pps.num_ref_idx_l1_default_active_minus1 + 1
    } else {
        0
    };
    if slice_type != SliceType::I && slice_type != SliceType::Si {
        let num_ref_idx_active_override_flag = r.read_bit()? != 0;
        if num_ref_idx_active_override_flag {
            num_ref_idx_l0_active = r.read_ue()? + 1;
            if slice_type == SliceType::B {
                num_ref_idx_l1_active = r.read_ue()? + 1;
            }
        }
    }

    // Spec 7.4.3: num_ref_idx_lX_active_minus1 is at most 15 for frames and
    // 31 for fields (the PPS defaults allow 31 either way)
    let max_refs = if field_pic_flag { 32 } else { 16 };
    if num_ref_idx_l0_active > max_refs || num_ref_idx_l1_active > max_refs {
        return Err("num_ref_idx_active out of range");
    }

    // ref_pic_list_modification (spec 7.3.3.1)
    let mut ref_list_mod_l0 = Vec::new();
    let mut ref_list_mod_l1 = Vec::new();
    if slice_type != SliceType::I && slice_type != SliceType::Si {
        let ref_pic_list_modification_flag_l0 = r.read_bit()? != 0;
        if ref_pic_list_modification_flag_l0 {
            loop {
                let idc = r.read_ue()?;
                if idc == 3 {
                    break;
                }
                // 4 and 5 exist only in MVC (Annex H) slices
                if idc > 3 {
                    return Err("modification_of_pic_nums_idc out of range");
                }
                let val = r.read_ue()?;
                ref_list_mod_l0.push((idc, val));
            }
        }
        if slice_type == SliceType::B {
            let ref_pic_list_modification_flag_l1 = r.read_bit()? != 0;
            if ref_pic_list_modification_flag_l1 {
                loop {
                    let idc = r.read_ue()?;
                    if idc == 3 {
                        break;
                    }
                    if idc > 3 {
                        return Err("modification_of_pic_nums_idc out of range");
                    }
                    let val = r.read_ue()?;
                    ref_list_mod_l1.push((idc, val));
                }
            }
        }
    }

    // pred_weight_table (spec 7.3.3.2)
    let needs_weight_table = (slice_type == SliceType::P && pps.weighted_pred_flag)
        || (slice_type == SliceType::B && pps.weighted_bipred_idc == 1);
    let weight_table = if needs_weight_table {
        let luma_log2_weight_denom = r.read_ue()?;
        // ChromaArrayType 0 (monochrome) has no chroma weights
        let has_chroma = sps.chroma_format_idc != 0;
        let chroma_log2_weight_denom = if has_chroma { r.read_ue()? } else { 0 };
        // Spec constrains these to [0, 7]. Clamp to prevent shift overflow.
        if luma_log2_weight_denom > 7 || chroma_log2_weight_denom > 7 {
            return Err("log2_weight_denom out of range");
        }
        let luma_def = 1i32 << luma_log2_weight_denom;
        let chroma_def = 1i32 << chroma_log2_weight_denom;

        // One reference's weights; spec 7.4.3.2 limits every weight and
        // offset to [-128, 127] (FFmpeg rejects the slice otherwise).
        let read_ref_weight = |r: &mut BitstreamReader| -> Result<RefWeight, &'static str> {
            let read_s8 = |r: &mut BitstreamReader| match r.read_se()? {
                v @ -128..=127 => Ok(v),
                _ => Err("prediction weight or offset out of range"),
            };
            let mut rw = RefWeight {
                luma_weight: luma_def,
                luma_offset: 0,
                chroma_weight: [chroma_def, chroma_def],
                chroma_offset: [0, 0],
            };
            if r.read_bit()? != 0 {
                rw.luma_weight = read_s8(r)?;
                rw.luma_offset = read_s8(r)?;
            }
            if has_chroma && r.read_bit()? != 0 {
                for j in 0..2 {
                    rw.chroma_weight[j] = read_s8(r)?;
                    rw.chroma_offset[j] = read_s8(r)?;
                }
            }
            Ok(rw)
        };

        let mut l0 = Vec::new();
        for _ in 0..num_ref_idx_l0_active {
            l0.push(read_ref_weight(&mut r)?);
        }

        let mut l1 = Vec::new();
        if slice_type == SliceType::B {
            for _ in 0..num_ref_idx_l1_active {
                l1.push(read_ref_weight(&mut r)?);
            }
        }

        Some(PredWeightTable {
            luma_log2_weight_denom,
            chroma_log2_weight_denom,
            l0,
            l1,
        })
    } else {
        None
    };

    // dec_ref_pic_marking (spec 7.3.3.3)
    let mut no_output_of_prior_pics_flag = false;
    let mut long_term_reference_flag = false;
    let mut mmco_ops: Vec<(u32, u32)> = Vec::new();
    let mut adaptive_ref_pic_marking = false;
    if nal_unit_type == NalUnitType::SliceIdr {
        no_output_of_prior_pics_flag = r.read_bit()? != 0;
        long_term_reference_flag = r.read_bit()? != 0;
    } else if nal_ref_idc > 0 {
        adaptive_ref_pic_marking = r.read_bit()? != 0;
        if adaptive_ref_pic_marking {
            loop {
                let op = r.read_ue()?;
                if op == 0 {
                    break;
                }
                match op {
                    // difference_of_pic_nums_minus1 is below MaxPicNum (up
                    // to 2^17 for fields); long-term indices and numbers
                    // are below 32 (spec 7.4.3.3, as FFmpeg checks)
                    1 => {
                        let diff = read_pic_num_diff(&mut r)?;
                        mmco_ops.push((1, diff));
                    }
                    2 => {
                        let long_term_pic_num = read_long_term_idx(&mut r)?;
                        mmco_ops.push((2, long_term_pic_num));
                    }
                    3 => {
                        let diff = read_pic_num_diff(&mut r)?;
                        let long_term_frame_idx = read_long_term_idx(&mut r)?;
                        mmco_ops.push((3, diff | (long_term_frame_idx << MMCO_DIFF_BITS)));
                    }
                    4 => {
                        let max_long_term_frame_idx_plus1 = r.read_ue()?;
                        if max_long_term_frame_idx_plus1 > 16 {
                            return Err("max_long_term_frame_idx_plus1 out of range");
                        }
                        mmco_ops.push((4, max_long_term_frame_idx_plus1));
                    }
                    5 => {
                        mmco_ops.push((5, 0));
                    }
                    6 => {
                        let long_term_frame_idx = read_long_term_idx(&mut r)?;
                        mmco_ops.push((6, long_term_frame_idx));
                    }
                    _ => return Err("memory_management_control_operation out of range"),
                }
            }
        }
    }

    // cabac_init_idc: parsed when entropy_coding_mode_flag=1 and slice is not I/SI
    // (spec 7.3.3: comes before slice_qp_delta). Spec 7.4.3 constrains the
    // value to the range [0, 2].
    let cabac_init_idc = if pps.entropy_coding_mode_flag
        && slice_type != SliceType::I
        && slice_type != SliceType::Si
    {
        let v = r.read_ue()?;
        if v > 2 {
            return Err("cabac_init_idc out of range");
        }
        v
    } else {
        0
    };

    let slice_qp_delta = r.read_se()?;
    // SliceQPY must lie in [-QpBdOffsetY, 51] (spec 7.4.3)
    let qp_bd_offset = 6 * sps.bit_depth_luma_minus8 as i64;
    let slice_qp = 26 + pps.pic_init_qp_minus26 as i64 + slice_qp_delta as i64;
    if !(-qp_bd_offset..=51).contains(&slice_qp) {
        return Err("slice QP out of range");
    }

    let mut disable_deblocking_filter_idc = 0;
    let mut slice_alpha_c0_offset_div2 = 0;
    let mut slice_beta_offset_div2 = 0;
    if pps.deblocking_filter_control_present_flag {
        disable_deblocking_filter_idc = r.read_ue()?;
        if disable_deblocking_filter_idc > 2 {
            return Err("disable_deblocking_filter_idc out of range");
        }
        if disable_deblocking_filter_idc != 1 {
            slice_alpha_c0_offset_div2 = r.read_se()?;
            slice_beta_offset_div2 = r.read_se()?;
            if !(-6..=6).contains(&slice_alpha_c0_offset_div2)
                || !(-6..=6).contains(&slice_beta_offset_div2)
            {
                return Err("deblocking filter offset out of range");
            }
        }
    }

    let header = SliceHeader {
        first_mb_in_slice,
        slice_type,
        pic_parameter_set_id,
        frame_num,
        idr_pic_id,
        redundant_pic_cnt,
        no_output_of_prior_pics_flag,
        long_term_reference_flag,
        slice_qp_delta,
        disable_deblocking_filter_idc,
        slice_alpha_c0_offset_div2,
        slice_beta_offset_div2,
        pic_order_cnt_lsb,
        delta_pic_order_cnt_bottom,
        delta_pic_order_cnt,
        num_ref_idx_l0_active,
        num_ref_idx_l1_active,
        direct_spatial_mv_pred_flag,
        mmco_ops,
        adaptive_ref_pic_marking,
        ref_list_mod_l0,
        ref_list_mod_l1,
        cabac_init_idc,
        weight_table,
        field_pic_flag,
        bottom_field_flag,
        mbaff_frame_flag,
    };

    Ok((header, r))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pps::parse_pps;
    use crate::sps::parse_sps;

    use crate::bitstream::BitWriter;

    /// SPS (4-bit frame_num, POC type 2, 16 refs) and PPS (deblocking control).
    fn params() -> (Sps, Pps) {
        let sps = parse_sps(&crate::sps::tests::baseline_sps(0, 0, 2, 16)).unwrap();
        let pps = parse_pps(&crate::pps::tests::pps_with(0, 0, 0, 0), None).unwrap();
        (sps, pps)
    }

    /// IDR I slice header with the given slice_qp_delta and deblocking offset.
    fn idr_header(slice_qp_delta: i32, alpha_offset_div2: i32) -> Vec<u8> {
        let mut w = BitWriter::default();
        w.ue(0).ue(7).ue(0).u(4, 0).ue(0).u(1, 0).u(1, 0);
        w.se(slice_qp_delta).ue(0).se(alpha_offset_div2).se(0);
        w.finish()
    }

    /// Non-IDR reference P slice header: `num_refs` active references and
    /// the given MMCO operations ((op, args)).
    fn p_header(num_refs: u32, mmcos: &[(u32, &[u32])]) -> Vec<u8> {
        let mut w = BitWriter::default();
        w.ue(0).ue(5).ue(0).u(4, 1).u(1, 1).ue(num_refs - 1).u(1, 0);
        w.u(1, 1);
        for &(op, args) in mmcos {
            w.ue(op);
            for &a in args {
                w.ue(a);
            }
        }
        w.ue(0).se(0).ue(1);
        w.finish()
    }

    fn parse(rbsp: &[u8], idr: bool) -> Result<SliceHeader, &'static str> {
        let (sps, pps) = params();
        let nal_type = if idr {
            NalUnitType::SliceIdr
        } else {
            NalUnitType::Slice
        };
        parse_slice_header(rbsp, &sps, &pps, nal_type, 1).map(|(h, _)| h)
    }

    /// Out-of-range slice header fields are rejected (spec 7.4.3), including
    /// values whose sums used to overflow (slice QP, deblocking offsets) or
    /// that padded the reference lists to billions of entries.
    #[test]
    fn test_slice_header_field_ranges() {
        assert!(parse(&idr_header(25, 6), true).is_ok());
        assert!(parse(&p_header(16, &[]), false).is_ok());
        for (rbsp, idr, what) in [
            (idr_header(26, 0), true, "slice QP"),
            (idr_header(i32::MAX, 0), true, "slice QP"),
            (idr_header(0, 7), true, "deblocking filter offset"),
            (p_header(17, &[]), false, "num_ref_idx_active"),
            (
                p_header(1, &[(3, &[0, 32])]),
                false,
                "long-term reference index",
            ),
            (
                p_header(1, &[(1, &[1 << 17])]),
                false,
                "difference_of_pic_nums_minus1",
            ),
            (
                p_header(1, &[(7, &[])]),
                false,
                "memory_management_control_operation",
            ),
        ] {
            assert!(parse(&rbsp, idr).unwrap_err().starts_with(what), "{what}");
        }
    }

    /// SP and SI slices are rejected as unsupported before the header fields
    /// only they carry, so they are never misparsed into another error.
    #[test]
    fn test_sp_si_slices_are_unsupported() {
        for slice_type in [3, 4, 8, 9] {
            let mut w = BitWriter::default();
            w.ue(0).ue(slice_type).ue(0).u(4, 0);
            let err = parse(&w.finish(), false).unwrap_err();
            assert!(matches!(
                crate::error::DecodeError::from(err),
                crate::error::DecodeError::Unsupported(_)
            ));
        }
    }

    /// MMCO 3 keeps a 17-bit difference_of_pic_nums_minus1 (field pictures
    /// with MaxFrameNum 2^16) apart from its long_term_frame_idx.
    #[test]
    fn test_mmco3_keeps_17_bit_pic_num_difference() {
        let header = parse(&p_header(1, &[(3, &[100_000, 5])]), false).unwrap();
        let (op, param) = header.mmco_ops[0];
        assert_eq!(op, 3);
        assert_eq!(param & ((1 << MMCO_DIFF_BITS) - 1), 100_000);
        assert_eq!(param >> MMCO_DIFF_BITS, 5);
    }
}
