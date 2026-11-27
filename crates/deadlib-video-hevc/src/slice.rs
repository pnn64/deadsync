//! HEVC slice segment header parsing (spec 7.3.6.1, semantics 7.4.7.1).
//!
//! A non-first slice segment with `dependent_slice_segment_flag = 1` has a
//! minimal header (just the slice address + byte alignment) and inherits
//! every other field from the most recent independent slice segment in the
//! picture. The parser returns a `SliceHeader` with the inherited fields
//! left at their defaults; the caller (`Decoder`) fills them in from the
//! saved independent slice header.

use crate::bitstream::BitstreamReader;
use crate::error::DecodeError;
use crate::nal::NalUnitType;
use crate::pps::Pps;
use crate::sps::{ShortTermRps, Sps, parse_st_ref_pic_set};

/// HEVC slice type (spec table 7-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceType {
    B = 0,
    P = 1,
    I = 2,
}

/// Per-slice long-term reference picture set (parsed from the slice header
/// when `long_term_ref_pics_present_flag = 1`).
///
/// For Phase 3d-1 we parse and store these fields but the decoder doesn't
/// yet use them (actual reference list construction lands in later phases).
#[derive(Debug, Clone, Default)]
pub struct LongTermRefPicSet {
    /// POC LSB for each long-term reference (either inherited from the SPS
    /// LT table via `lt_idx_sps` or signaled inline as `poc_lsb_lt[i]`).
    pub poc_lsb_lt: Vec<u32>,
    /// `used_by_curr_pic_lt_flag[i]`.
    pub used_by_curr_pic_lt_flag: Vec<bool>,
    /// Optional POC MSB delta, per spec 7.4.7.1 "long-term refs".
    pub delta_poc_msb_present_flag: Vec<bool>,
    pub delta_poc_msb_cycle_lt: Vec<u32>,
    /// `num_long_term_sps`: the first this many entries come from the
    /// SPS candidates, the rest are coded in the slice header (the MSB
    /// cycle accumulation restarts at the first of those, eq. 7-52).
    pub num_long_term_sps: usize,
}

/// Parsed `ref_pic_list_modification()` syntax (spec 7.3.6.2).
///
/// When `ref_pic_list_modification_flag_l{0,1}` is false, the corresponding
/// `list_entry_l{0,1}` is empty and the temp list is used unchanged. When
/// the flag is true, the list has exactly `num_ref_idx_l{0,1}_active_minus1
/// + 1` entries, each an index into the temp list.
#[derive(Debug, Clone, Default)]
pub struct RefPicListModification {
    pub ref_pic_list_modification_flag_l0: bool,
    pub list_entry_l0: Vec<u32>,
    pub ref_pic_list_modification_flag_l1: bool,
    pub list_entry_l1: Vec<u32>,
}

/// Maximum number of reference pictures per list for weight table storage.
const MAX_REFS: usize = 16;

/// Prediction weight table (spec 7.3.6.3 / 7.4.6.3).
///
/// Weights are stored in the "effective" form used by FFmpeg:
///   `weight = (1 << log2_weight_denom) + delta_weight`
/// with offset as-is for luma, and with the chroma offset already adjusted
/// per spec equation 7-59 (the `- (128 * weight >> denom) + 128` formula).
///
/// Default (when the `luma_weight_flag` / `chroma_weight_flag` is 0):
///   weight = `1 << log2_weight_denom`, offset = 0.
#[derive(Debug, Clone)]
pub struct PredWeightTable {
    pub luma_log2_weight_denom: u8,
    pub chroma_log2_weight_denom: u8,
    pub luma_weight_l0: [i16; MAX_REFS],
    pub luma_offset_l0: [i16; MAX_REFS],
    pub chroma_weight_l0: [[i16; 2]; MAX_REFS],
    pub chroma_offset_l0: [[i16; 2]; MAX_REFS],
    pub luma_weight_l1: [i16; MAX_REFS],
    pub luma_offset_l1: [i16; MAX_REFS],
    pub chroma_weight_l1: [[i16; 2]; MAX_REFS],
    pub chroma_offset_l1: [[i16; 2]; MAX_REFS],
    /// high_precision_offsets_enabled_flag: offsets are at the sample bit
    /// depth and are not scaled by `1 << (BitDepth - 8)` (spec 7.4.7.3).
    pub high_precision_offsets: bool,
}

impl Default for PredWeightTable {
    fn default() -> Self {
        Self {
            luma_log2_weight_denom: 0,
            chroma_log2_weight_denom: 0,
            luma_weight_l0: [0; MAX_REFS],
            luma_offset_l0: [0; MAX_REFS],
            chroma_weight_l0: [[0; 2]; MAX_REFS],
            chroma_offset_l0: [[0; 2]; MAX_REFS],
            luma_weight_l1: [0; MAX_REFS],
            luma_offset_l1: [0; MAX_REFS],
            chroma_weight_l1: [[0; 2]; MAX_REFS],
            chroma_offset_l1: [[0; 2]; MAX_REFS],
            high_precision_offsets: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SliceHeader {
    pub first_slice_segment_in_pic_flag: bool,
    // Parsed for spec completeness; not consumed by the decoder yet.
    #[allow(dead_code)]
    pub no_output_of_prior_pics_flag: bool,
    // Parsed for spec completeness; not consumed by the decoder yet.
    #[allow(dead_code)]
    pub slice_pic_parameter_set_id: u32,
    pub dependent_slice_segment_flag: bool,
    pub slice_segment_address: u32,
    pub slice_type: SliceType,
    pub pic_output_flag: bool,
    /// Picture order count LSB. 0 for IDR pictures (not coded in the stream).
    pub slice_pic_order_cnt_lsb: u32,
    pub slice_sao_luma_flag: bool,
    pub slice_sao_chroma_flag: bool,
    pub slice_qp_delta: i32,
    /// Effective slice QP: `pps.init_qp + slice_qp_delta`. Spec eq. 7-54.
    pub slice_qp_y: i32,
    /// Slice-level Cb QP offset (spec 7.4.7.1). Summed with
    /// `pps.pps_cb_qp_offset` when deriving chroma QP.
    pub slice_cb_qp_offset: i32,
    /// Slice-level Cr QP offset.
    pub slice_cr_qp_offset: i32,
    /// Per-slice flag enabling per-CU chroma QP offset signaling (spec 7.3.6.1).
    /// Only present if `pps.chroma_qp_offset_list_enabled_flag = 1`.
    pub cu_chroma_qp_offset_enabled_flag: bool,
    /// Slice-level deblocking disable flag. Inherits from PPS unless an
    /// override is signaled (Phase 3b-1 only supports the inherited path).
    pub slice_deblocking_filter_disabled_flag: bool,
    /// Slice-level β offset (per spec, in `2 *` units when applied).
    pub slice_beta_offset_div2: i32,
    /// Slice-level tc offset (per spec, in `2 *` units when applied).
    pub slice_tc_offset_div2: i32,
    /// Phase 3c-3 (WPP): cumulative byte offsets (relative to the start of
    /// the slice data, i.e. the first byte after the slice header's byte
    /// alignment) where each WPP substream / tile substream starts. The
    /// i-th entry is the byte offset of substream `i + 1`; substream 0
    /// always starts at offset 0.
    ///
    /// Empty for non-WPP / non-tiles slices. For WPP, the length equals
    /// `num_entry_point_offsets` which (for a single slice) equals
    /// `num_ctb_rows_in_slice - 1` when WPP is enabled.
    pub entry_point_offsets: Vec<u32>,
    /// Number of bits consumed by the slice header so we know where the
    /// slice data (CABAC bytestream) begins.
    pub header_size_bits: usize,

    // --- Phase 3d-1 inter bitstream fields (parsed but not yet used for
    // actual decoding) ---
    /// `short_term_ref_pic_set_sps_flag`. When true, the slice's ST RPS is
    /// `sps.st_ref_pic_sets[short_term_ref_pic_set_idx]`. When false, the
    /// slice defined its own inline RPS in `short_term_rps`.
    pub short_term_ref_pic_set_sps_flag: bool,
    /// Index into `sps.st_ref_pic_sets` when
    /// `short_term_ref_pic_set_sps_flag == true`; otherwise unused.
    pub short_term_ref_pic_set_idx: u32,
    /// Inline ST RPS parsed from the slice header when
    /// `short_term_ref_pic_set_sps_flag == false`. `None` for IDR slices
    /// (no RPS is coded) or when the flag was set.
    pub short_term_rps: Option<ShortTermRps>,
    /// Parsed long-term RPS for the slice. Empty for IDR slices or SPS
    /// without `long_term_ref_pics_present_flag`.
    pub long_term_rps: LongTermRefPicSet,
    /// `slice_temporal_mvp_enabled_flag`. Only parsed when
    /// `sps.sps_temporal_mvp_enabled_flag = 1`. Defaults to false.
    pub slice_temporal_mvp_enabled_flag: bool,
    /// `num_ref_idx_l0_active_minus1` after override (or the PPS default).
    /// Always >= 0 for P/B slices, 0 otherwise.
    pub num_ref_idx_l0_active_minus1: u32,
    /// `num_ref_idx_l1_active_minus1` after override (or the PPS default).
    /// Only meaningful for B slices.
    pub num_ref_idx_l1_active_minus1: u32,
    /// Phase 3d-1: parsed but not used.
    pub mvd_l1_zero_flag: bool,
    /// CABAC init flag (when `pps.cabac_init_present_flag && P/B slice`).
    pub cabac_init_flag: bool,
    /// For temporal MVP: which reference list holds the collocated picture
    /// (0 = L0, 1 = L1). In P slices it's always 0.
    pub collocated_from_l0_flag: bool,
    /// For temporal MVP: the collocated reference index inside the chosen list.
    pub collocated_ref_idx: u32,
    /// `MaxNumMergeCand = 5 - five_minus_max_num_merge_cand`. Defaults to 5
    /// for I slices (where it's not signaled) to avoid any "unset" sentinel.
    pub max_num_merge_cand: u32,
    /// Parsed `ref_pic_list_modification()` syntax (spec 7.3.6.2). Defaults
    /// to all-disabled when not coded (I slices, `NumPocTotalCurr <= 1`, or
    /// `lists_modification_present_flag = 0`), matching the spec's inferred
    /// values.
    pub ref_pic_list_modification: RefPicListModification,
    /// Weighted prediction table — populated when `weighted_pred_flag` (P) or
    /// `weighted_bipred_flag` (B) is set in the PPS. Otherwise all weights are
    /// `1 << denom` and all offsets are 0.
    pub pred_weight_table: PredWeightTable,
    /// NAL unit type copy — needed for POC derivation downstream.
    pub nal_unit_type: NalUnitType,
    /// `slice_loop_filter_across_slices_enabled_flag`. When false, deblocking
    /// and SAO are suppressed at slice boundaries. Inferred equal to
    /// `pps.pps_loop_filter_across_slices_enabled_flag` when not present.
    pub slice_loop_filter_across_slices_enabled_flag: bool,
    /// NAL temporal id — used by the DPB's "prev_tid0" tracking. Always 0
    /// for fixtures we have today; populated by `Decoder::decode_slice` on
    /// behalf of the slice since the parser doesn't see the NAL header.
    pub temporal_id: u8,
    /// Phase 3d-1: computed picture POC. For IDR slices this is 0; for
    /// non-IDR slices it's filled in by `Decoder::decode_slice` via
    /// `compute_poc`. The parser leaves it at the placeholder value
    /// (signed i32 equivalent of `slice_pic_order_cnt_lsb`) and the
    /// decoder overwrites it.
    pub poc: i32,
}

/// Parse the slice segment header for a single VCL NAL unit.
///
/// `nal_unit_type` is needed because IDR slices skip POC and RPS parsing.
/// `sps` and `pps` provide the gating fields (SAO, deblock, tiles, ...).
pub fn parse_slice_segment_header(
    rbsp: &[u8],
    nal_unit_type: NalUnitType,
    sps: &Sps,
    pps: &Pps,
) -> Result<SliceHeader, DecodeError> {
    let mut r = BitstreamReader::new(rbsp);

    let first_slice_segment_in_pic_flag = r.read_bit()? == 1;

    let mut no_output_of_prior_pics_flag = false;
    if nal_unit_type.is_irap() {
        no_output_of_prior_pics_flag = r.read_bit()? == 1;
    }

    let slice_pic_parameter_set_id = r.read_ue()?;
    if slice_pic_parameter_set_id != pps.pps_pic_parameter_set_id {
        return Err(DecodeError::InvalidSyntax(
            "slice_pic_parameter_set_id does not match active PPS",
        ));
    }

    // spec 7.3.6.1: dependent_slice_segment_flag and slice_segment_address
    // are only present when first_slice_segment_in_pic_flag == 0. A
    // dependent slice segment carries a minimal header: only the slice
    // address + byte alignment. Every other field is inherited from the
    // most recent independent slice segment in the picture by the caller.
    let mut dependent_slice_segment_flag = false;
    let mut slice_segment_address = 0u32;
    if !first_slice_segment_in_pic_flag {
        if pps.dependent_slice_segments_enabled_flag {
            dependent_slice_segment_flag = r.read_bit()? == 1;
        }
        // slice_segment_address: ceil(log2(NumCtbsInPic)) bits. Spec eq. 7-78.
        let num_ctbs_in_pic = sps.pic_width_in_ctbs_y() * sps.pic_height_in_ctbs_y();
        let slice_address_length = ceil_log2(num_ctbs_in_pic) as u8;
        slice_segment_address = if slice_address_length > 0 {
            r.read_bits(slice_address_length)?
        } else {
            0
        };
        if slice_segment_address >= num_ctbs_in_pic {
            return Err(DecodeError::InvalidSyntax(
                "slice_segment_address out of range",
            ));
        }
    }

    // Dependent slice segment: skip the big `if (!dependent_slice_segment_flag)`
    // body entirely — every field in it is inherited from the most recent
    // independent slice segment and filled in by the caller. Spec 7.3.6.1
    // explicitly gates the reserved-bits / slice_type / ... region on
    // `!dependent_slice_segment_flag`, but the entry_point_offsets section
    // below and the slice_header_extension + byte_alignment() tail are
    // parsed regardless.
    if !dependent_slice_segment_flag {
        // Reserved slice header bits (none for our PPS).
        for _ in 0..pps.num_extra_slice_header_bits {
            let _ = r.read_bit()?;
        }
    }

    // Inherited-from-independent-segment fields. For a dependent slice
    // segment they stay at their default values here and are filled in by
    // the caller from the saved independent slice header. For an
    // independent slice segment they are decoded from the bitstream.
    let mut slice_type = SliceType::I;
    let mut pic_output_flag = true;
    let mut slice_pic_order_cnt_lsb = 0u32;
    let mut slice_sao_luma_flag = false;
    let mut slice_sao_chroma_flag = false;
    let mut slice_qp_delta = 0i32;
    let mut slice_qp_y = 0i32;
    let mut slice_cb_qp_offset = 0i32;
    let mut slice_cr_qp_offset = 0i32;
    let mut cu_chroma_qp_offset_enabled_flag = false;
    let mut slice_deblocking_filter_disabled_flag = pps.pps_deblocking_filter_disabled_flag;
    // Inferred from the PPS unless the slice overrides them (spec 7.4.7.1).
    let mut slice_beta_offset_div2 = pps.pps_beta_offset_div2;
    let mut slice_tc_offset_div2 = pps.pps_tc_offset_div2;
    let mut slice_loop_filter_across_slices_enabled_flag =
        pps.pps_loop_filter_across_slices_enabled_flag;

    // Phase 3d-1 inter bitstream fields. Defaults match what an I-slice
    // picture expects: no RPS entries, no temporal MVP, zero ref indices.
    let mut short_term_ref_pic_set_sps_flag = false;
    let mut short_term_ref_pic_set_idx: u32 = 0;
    let mut short_term_rps: Option<ShortTermRps> = None;
    let mut long_term_rps = LongTermRefPicSet::default();
    let mut slice_temporal_mvp_enabled_flag = false;
    let mut num_ref_idx_l0_active_minus1: u32 = pps.num_ref_idx_l0_default_active_minus1;
    let mut num_ref_idx_l1_active_minus1: u32 = pps.num_ref_idx_l1_default_active_minus1;
    let mut mvd_l1_zero_flag = false;
    let mut cabac_init_flag = false;
    let mut collocated_from_l0_flag = true;
    let mut collocated_ref_idx: u32 = 0;
    let mut max_num_merge_cand: u32 = 5;
    let mut pred_weight_table = PredWeightTable::default();
    let mut ref_pic_list_modification = RefPicListModification::default();

    if !dependent_slice_segment_flag {
        let slice_type_raw = r.read_ue()?;
        slice_type = match slice_type_raw {
            0 => SliceType::B,
            1 => SliceType::P,
            2 => SliceType::I,
            _ => {
                return Err(DecodeError::InvalidSyntax("invalid slice_type"));
            }
        };
        // 7.4.7.1: the slices of an IRAP picture are I slices.
        if nal_unit_type.is_irap() && slice_type != SliceType::I {
            return Err(DecodeError::InvalidSyntax("inter slice in an IRAP picture"));
        }

        if pps.output_flag_present_flag {
            pic_output_flag = r.read_bit()? == 1;
        }

        // separate_colour_plane_flag is gated by chroma_format_idc==3, which we
        // already rejected in SPS parsing.

        // Phase 3d-1: POC LSB + RPS parsing for non-IDR pictures. IDR slices
        // still skip this whole section (POC forced to 0).
        if !nal_unit_type.is_idr() {
            slice_pic_order_cnt_lsb = r.read_bits(sps.log2_max_pic_order_cnt_lsb)?;

            short_term_ref_pic_set_sps_flag = r.read_bit()? == 1;
            if !short_term_ref_pic_set_sps_flag {
                // Inline ST RPS. Index passed is num_short_term_ref_pic_sets
                // per spec, so the slice RPS can use any of the SPS RPSs as
                // a reference via delta_idx.
                let rps = parse_st_ref_pic_set(
                    &mut r,
                    sps.num_short_term_ref_pic_sets as usize,
                    sps.num_short_term_ref_pic_sets as usize,
                    &sps.st_ref_pic_sets,
                )?;
                short_term_rps = Some(rps);
            } else {
                // Selected by index. Width is ceil(log2(num_short_term_ref_pic_sets));
                // zero bits when only one RPS is defined.
                if sps.num_short_term_ref_pic_sets == 0 {
                    return Err(DecodeError::InvalidSyntax(
                        "short_term_ref_pic_set_sps_flag set but SPS has no ST-RPSs",
                    ));
                }
                let numbits = ceil_log2(sps.num_short_term_ref_pic_sets) as u8;
                short_term_ref_pic_set_idx = if numbits > 0 {
                    r.read_bits(numbits)?
                } else {
                    0
                };
                if short_term_ref_pic_set_idx >= sps.num_short_term_ref_pic_sets {
                    return Err(DecodeError::InvalidSyntax(
                        "short_term_ref_pic_set_idx out of range",
                    ));
                }
            }

            if sps.long_term_ref_pics_present_flag {
                long_term_rps = parse_lt_ref_pic_set(&mut r, sps)?;
            }
            // 7.4.7.1: the short-term and long-term pictures together fit
            // in the DPB (every one may be generated if missing, 8.3.3).
            let st_pics = match &short_term_rps {
                Some(rps) => rps.num_delta_pocs(),
                None => sps
                    .st_ref_pic_sets
                    .get(short_term_ref_pic_set_idx as usize)
                    .map_or(0, ShortTermRps::num_delta_pocs),
            };
            if st_pics + long_term_rps.poc_lsb_lt.len()
                > sps.sps_max_dec_pic_buffering_minus1 as usize
            {
                return Err(DecodeError::InvalidSyntax(
                    "reference picture set larger than the DPB",
                ));
            }

            if sps.sps_temporal_mvp_enabled_flag {
                slice_temporal_mvp_enabled_flag = r.read_bit()? == 1;
            }
        }

        if sps.sample_adaptive_offset_enabled_flag {
            slice_sao_luma_flag = r.read_bit()? == 1;
            // For 4:2:0 there's also a chroma SAO flag.
            slice_sao_chroma_flag = r.read_bit()? == 1;
        }

        // Phase 3d-1: P/B-slice inter section (only parsing, no decoding).
        if slice_type != SliceType::I {
            let num_ref_idx_active_override_flag = r.read_bit()? == 1;
            if num_ref_idx_active_override_flag {
                num_ref_idx_l0_active_minus1 = r.read_ue()?;
                if slice_type == SliceType::B {
                    num_ref_idx_l1_active_minus1 = r.read_ue()?;
                }
            }
            if num_ref_idx_l0_active_minus1 >= 15 || num_ref_idx_l1_active_minus1 >= 15 {
                return Err(DecodeError::InvalidSyntax(
                    "num_ref_idx_l{0,1}_active_minus1 out of range",
                ));
            }

            // Phase 3d-2: `ref_pic_list_modification()` parsing (spec
            // 7.3.6.2). Only signaled when
            // `pps.lists_modification_present_flag = 1` AND
            // `NumPocTotalCurr > 1`. Matches FFmpeg's `hevcdec.c`
            // `lists_modification_present_flag && nb_refs > 1` guard.
            //
            // `NumPocTotalCurr` is the total number of "current" refs in
            // the resolved RPS = st_curr_before + st_curr_after + lt_curr.
            // We compute it here from the already-resolved ST RPS (either
            // SPS-indexed or inline) plus the slice's LT RPS.
            if pps.lists_modification_present_flag {
                let st_rps: Option<&ShortTermRps> = if short_term_ref_pic_set_sps_flag {
                    sps.st_ref_pic_sets.get(short_term_ref_pic_set_idx as usize)
                } else {
                    short_term_rps.as_ref()
                };
                let mut num_poc_total_curr: u32 = 0;
                if let Some(st) = st_rps {
                    for f in &st.used_by_curr_pic_s0_flag {
                        if *f {
                            num_poc_total_curr += 1;
                        }
                    }
                    for f in &st.used_by_curr_pic_s1_flag {
                        if *f {
                            num_poc_total_curr += 1;
                        }
                    }
                }
                for f in &long_term_rps.used_by_curr_pic_lt_flag {
                    if *f {
                        num_poc_total_curr += 1;
                    }
                }

                if num_poc_total_curr > 1 {
                    let nb_bits = ceil_log2(num_poc_total_curr) as u8;
                    let flag_l0 = r.read_bit()? == 1;
                    ref_pic_list_modification.ref_pic_list_modification_flag_l0 = flag_l0;
                    if flag_l0 {
                        let count = num_ref_idx_l0_active_minus1 + 1;
                        ref_pic_list_modification
                            .list_entry_l0
                            .reserve(count as usize);
                        for _ in 0..count {
                            let entry = if nb_bits > 0 {
                                r.read_bits(nb_bits)?
                            } else {
                                0
                            };
                            if entry >= num_poc_total_curr {
                                return Err(DecodeError::InvalidSyntax(
                                    "list_entry_l0 out of range",
                                ));
                            }
                            ref_pic_list_modification.list_entry_l0.push(entry);
                        }
                    }
                    if slice_type == SliceType::B {
                        let flag_l1 = r.read_bit()? == 1;
                        ref_pic_list_modification.ref_pic_list_modification_flag_l1 = flag_l1;
                        if flag_l1 {
                            let count = num_ref_idx_l1_active_minus1 + 1;
                            ref_pic_list_modification
                                .list_entry_l1
                                .reserve(count as usize);
                            for _ in 0..count {
                                let entry = if nb_bits > 0 {
                                    r.read_bits(nb_bits)?
                                } else {
                                    0
                                };
                                if entry >= num_poc_total_curr {
                                    return Err(DecodeError::InvalidSyntax(
                                        "list_entry_l1 out of range",
                                    ));
                                }
                                ref_pic_list_modification.list_entry_l1.push(entry);
                            }
                        }
                    }
                }
            }

            if slice_type == SliceType::B {
                mvd_l1_zero_flag = r.read_bit()? == 1;
            }
            if pps.cabac_init_present_flag {
                cabac_init_flag = r.read_bit()? == 1;
            }
            if slice_temporal_mvp_enabled_flag {
                if slice_type == SliceType::B {
                    collocated_from_l0_flag = r.read_bit()? == 1;
                } else {
                    // P slice: always L0.
                    collocated_from_l0_flag = true;
                }
                // Whether we read collocated_ref_idx depends on the number
                // of refs in the selected list. Follow FFmpeg's signaling
                // rule: `nb_refs[collocated_list] > 1`.
                let active_list_max = if collocated_from_l0_flag {
                    num_ref_idx_l0_active_minus1
                } else {
                    num_ref_idx_l1_active_minus1
                };
                if active_list_max > 0 {
                    collocated_ref_idx = r.read_ue()?;
                    if collocated_ref_idx > active_list_max {
                        return Err(DecodeError::InvalidSyntax(
                            "collocated_ref_idx out of range",
                        ));
                    }
                }
            }

            if (pps.weighted_pred_flag && slice_type == SliceType::P)
                || (pps.weighted_bipred_flag && slice_type == SliceType::B)
            {
                pred_weight_table = parse_pred_weight_table(
                    &mut r,
                    slice_type,
                    num_ref_idx_l0_active_minus1 + 1,
                    num_ref_idx_l1_active_minus1 + 1,
                    sps.chroma_format_idc,
                    sps,
                )?;
            }

            let five_minus_max_num_merge_cand = r.read_ue()?;
            if five_minus_max_num_merge_cand > 4 {
                return Err(DecodeError::InvalidSyntax(
                    "five_minus_max_num_merge_cand out of range",
                ));
            }
            max_num_merge_cand = 5 - five_minus_max_num_merge_cand;
        }

        slice_qp_delta = r.read_se()?;
        // 7.4.7.1: SliceQpY is in -QpBdOffsetY..=51.
        let qp_bd_offset_y = 6 * (sps.bit_depth_luma as i32 - 8);
        slice_qp_y = pps
            .init_qp
            .checked_add(slice_qp_delta)
            .filter(|qp| (-qp_bd_offset_y..=51).contains(qp))
            .ok_or(DecodeError::InvalidSyntax("SliceQpY out of range"))?;

        if pps.pps_slice_chroma_qp_offsets_present_flag {
            slice_cb_qp_offset = r.read_se()?;
            slice_cr_qp_offset = r.read_se()?;
            // Each, and each added to the PPS offset, in -12..=12.
            let in_range =
                |o: i32, p: i32| (-12..=12).contains(&o) && (-12..=12).contains(&(o + p));
            if !in_range(slice_cb_qp_offset, pps.pps_cb_qp_offset)
                || !in_range(slice_cr_qp_offset, pps.pps_cr_qp_offset)
            {
                return Err(DecodeError::InvalidSyntax(
                    "slice_cb/cr_qp_offset out of range",
                ));
            }
        }

        if pps.chroma_qp_offset_list_enabled_flag {
            cu_chroma_qp_offset_enabled_flag = r.read_bit()? == 1;
        }

        if pps.deblocking_filter_override_enabled_flag {
            let deblocking_filter_override_flag = r.read_bit()? == 1;
            if deblocking_filter_override_flag {
                slice_deblocking_filter_disabled_flag = r.read_bit()? == 1;
                if !slice_deblocking_filter_disabled_flag {
                    slice_beta_offset_div2 = r.read_se()?;
                    slice_tc_offset_div2 = r.read_se()?;
                    if !(-6..=6).contains(&slice_beta_offset_div2)
                        || !(-6..=6).contains(&slice_tc_offset_div2)
                    {
                        return Err(DecodeError::InvalidSyntax(
                            "slice_beta/tc_offset_div2 out of range",
                        ));
                    }
                }
            }
        }

        if pps.pps_loop_filter_across_slices_enabled_flag
            && (slice_sao_luma_flag
                || slice_sao_chroma_flag
                || !slice_deblocking_filter_disabled_flag)
        {
            slice_loop_filter_across_slices_enabled_flag = r.read_bit()? == 1;
        }
    }

    // Phase 3c-3: WPP entry point parsing. `tiles_enabled_flag` is still
    // rejected at PPS parse time, so this branch currently only fires for
    // `entropy_coding_sync_enabled_flag = 1`.
    let mut entry_point_offsets: Vec<u32> = Vec::new();
    if pps.tiles_enabled_flag || pps.entropy_coding_sync_enabled_flag {
        let num_entry_point_offsets = r.read_ue()?;
        // 7.4.7.1: one substream per CTB row (WPP), per tile, or per CTB
        // row of each tile column (both).
        let h_ctbs = sps.pic_height_in_ctbs_y() as usize;
        let (cols, rows) = (pps.num_tile_columns, pps.num_tile_rows);
        let max_substreams = match (pps.tiles_enabled_flag, pps.entropy_coding_sync_enabled_flag) {
            (true, true) => cols * h_ctbs,
            (true, false) => cols * rows,
            _ => h_ctbs,
        };
        if num_entry_point_offsets as usize >= max_substreams.max(1) {
            return Err(DecodeError::InvalidSyntax(
                "num_entry_point_offsets too large",
            ));
        }
        if num_entry_point_offsets > 0 {
            let offset_len_minus1 = r.read_ue()?;
            if offset_len_minus1 >= 32 {
                return Err(DecodeError::InvalidSyntax(
                    "offset_len_minus1 out of range [0, 31]",
                ));
            }
            let offset_len = (offset_len_minus1 + 1) as u8;
            entry_point_offsets.reserve(num_entry_point_offsets as usize);
            let mut cumulative: u32 = 0;
            for _ in 0..num_entry_point_offsets {
                let v = r.read_bits(offset_len)?;
                // Each entry is the substream byte length minus one; the
                // stored "entry point" is the cumulative byte offset of the
                // next substream from the start of the slice data. So
                // substream 0 starts at 0, substream 1 starts at
                // (entry_point_offset_minus1[0] + 1), substream 2 starts at
                // (entry_point_offset_minus1[0] + 1) +
                // (entry_point_offset_minus1[1] + 1), etc.
                cumulative = v
                    .checked_add(1)
                    .and_then(|d| cumulative.checked_add(d))
                    .ok_or(DecodeError::InvalidSyntax("entry_point_offset overflow"))?;
                entry_point_offsets.push(cumulative);
            }
        }
    }

    if pps.slice_segment_header_extension_present_flag {
        // Reserved for future extensions (spec 7.3.6.1 / 7.4.7.1): a length in
        // 0..=256 followed by that many bytes, which decoders shall ignore.
        let slice_segment_header_extension_length = r.read_ue()?;
        if slice_segment_header_extension_length > 256 {
            return Err(DecodeError::InvalidSyntax(
                "slice_segment_header_extension_length out of range",
            ));
        }
        for _ in 0..slice_segment_header_extension_length {
            r.read_bits(8)?;
        }
    }

    // byte_alignment(): one '1' bit followed by zero bits up to a byte boundary.
    // Spec 7.3.2.11.
    let one_bit = r.read_bit()?;
    if one_bit != 1 {
        return Err(DecodeError::InvalidSyntax("byte_alignment: expected 1 bit"));
    }
    while !at_byte_boundary(&r) {
        let zero_bit = r.read_bit()?;
        if zero_bit != 0 {
            return Err(DecodeError::InvalidSyntax("byte_alignment: expected 0 bit"));
        }
    }

    let (byte_offset, bit_offset) = r.position();
    debug_assert_eq!(bit_offset, 0, "byte_alignment should be byte-aligned");
    let header_size_bits = byte_offset * 8;

    Ok(SliceHeader {
        first_slice_segment_in_pic_flag,
        no_output_of_prior_pics_flag,
        slice_pic_parameter_set_id,
        dependent_slice_segment_flag,
        slice_segment_address,
        slice_type,
        pic_output_flag,
        slice_pic_order_cnt_lsb,
        slice_sao_luma_flag,
        slice_sao_chroma_flag,
        slice_qp_delta,
        slice_qp_y,
        slice_cb_qp_offset,
        slice_cr_qp_offset,
        cu_chroma_qp_offset_enabled_flag,
        slice_deblocking_filter_disabled_flag,
        slice_beta_offset_div2,
        slice_tc_offset_div2,
        entry_point_offsets,
        header_size_bits,
        short_term_ref_pic_set_sps_flag,
        short_term_ref_pic_set_idx,
        short_term_rps,
        long_term_rps,
        slice_temporal_mvp_enabled_flag,
        num_ref_idx_l0_active_minus1,
        num_ref_idx_l1_active_minus1,
        mvd_l1_zero_flag,
        cabac_init_flag,
        collocated_from_l0_flag,
        collocated_ref_idx,
        max_num_merge_cand,
        ref_pic_list_modification,
        pred_weight_table,
        slice_loop_filter_across_slices_enabled_flag,
        nal_unit_type,
        temporal_id: 0,
        // IDR pictures have POC = 0 regardless of slice_pic_order_cnt_lsb
        // (which is actually not even coded for IDR). For non-IDR slices
        // the decoder will overwrite this with the full computed POC via
        // `compute_poc` once it has access to prev_tid0_poc.
        poc: 0,
    })
}

/// Parse `lt_ref_pic_set()` per spec 7.3.6.1 (semantics 7.4.7.1). Called
/// only when `sps.long_term_ref_pics_present_flag` is true and the slice
/// is non-IDR.
fn parse_lt_ref_pic_set(
    r: &mut BitstreamReader,
    sps: &Sps,
) -> Result<LongTermRefPicSet, DecodeError> {
    let num_long_term_sps = if sps.num_long_term_ref_pics_sps > 0 {
        r.read_ue()?
    } else {
        0
    };
    let num_long_term_pics = r.read_ue()?;
    if num_long_term_sps > sps.num_long_term_ref_pics_sps {
        return Err(DecodeError::InvalidSyntax(
            "num_long_term_sps exceeds SPS count",
        ));
    }
    let total = num_long_term_sps.saturating_add(num_long_term_pics);
    if total as usize > crate::sps::MAX_LONG_TERM_REF_PICS_SPS * 2 {
        return Err(DecodeError::InvalidSyntax(
            "long-term reference count out of range",
        ));
    }

    let mut set = LongTermRefPicSet {
        poc_lsb_lt: Vec::with_capacity(total as usize),
        used_by_curr_pic_lt_flag: Vec::with_capacity(total as usize),
        delta_poc_msb_present_flag: Vec::with_capacity(total as usize),
        delta_poc_msb_cycle_lt: Vec::with_capacity(total as usize),
        num_long_term_sps: num_long_term_sps as usize,
    };

    for i in 0..total {
        if i < num_long_term_sps {
            let lt_idx_sps = if sps.num_long_term_ref_pics_sps > 1 {
                let numbits = ceil_log2(sps.num_long_term_ref_pics_sps) as u8;
                r.read_bits(numbits)? as usize
            } else {
                0usize
            };
            if lt_idx_sps >= sps.lt_ref_pic_poc_lsb_sps.len() {
                return Err(DecodeError::InvalidSyntax("lt_idx_sps out of range"));
            }
            set.poc_lsb_lt.push(sps.lt_ref_pic_poc_lsb_sps[lt_idx_sps]);
            set.used_by_curr_pic_lt_flag
                .push(sps.used_by_curr_pic_lt_sps_flag[lt_idx_sps]);
        } else {
            set.poc_lsb_lt
                .push(r.read_bits(sps.log2_max_pic_order_cnt_lsb)?);
            set.used_by_curr_pic_lt_flag.push(r.read_bit()? == 1);
        }
        let msb_present = r.read_bit()? == 1;
        set.delta_poc_msb_present_flag.push(msb_present);
        if msb_present {
            set.delta_poc_msb_cycle_lt.push(r.read_ue()?);
        } else {
            set.delta_poc_msb_cycle_lt.push(0);
        }
    }
    Ok(set)
}

/// Parse `pred_weight_table()` (spec 7.3.6.3) and return the populated table.
fn parse_pred_weight_table(
    r: &mut BitstreamReader,
    slice_type: SliceType,
    nb_ref_l0: u32,
    nb_ref_l1: u32,
    chroma_format_idc: u32,
    sps: &Sps,
) -> Result<PredWeightTable, DecodeError> {
    let mut wt = PredWeightTable {
        high_precision_offsets: sps.range.high_precision_offsets_enabled_flag,
        ..Default::default()
    };
    // WpOffsetHalfRangeC (spec 7.4.7.3): 1 << 7, or 1 << (BitDepthC - 1)
    // with high-precision offsets.
    let half_c: i64 = if wt.high_precision_offsets {
        1 << (sps.bit_depth_chroma - 1)
    } else {
        128
    };

    let luma_log2_weight_denom = r.read_ue()?;
    if luma_log2_weight_denom > 7 {
        return Err(DecodeError::InvalidSyntax("luma_log2_weight_denom > 7"));
    }
    wt.luma_log2_weight_denom = luma_log2_weight_denom as u8;

    let mut chroma_log2_weight_denom = luma_log2_weight_denom as i32;
    if chroma_format_idc != 0 {
        // delta_chroma_log2_weight_denom can be near i32::MAX in a damaged
        // header: add in i64 before the range check, as FFmpeg does.
        let denom = i64::from(chroma_log2_weight_denom) + i64::from(r.read_se()?);
        if !(0..=7).contains(&denom) {
            return Err(DecodeError::InvalidSyntax(
                "chroma_log2_weight_denom out of range",
            ));
        }
        chroma_log2_weight_denom = denom as i32;
    }
    wt.chroma_log2_weight_denom = chroma_log2_weight_denom as u8;

    // 7.4.7.3: delta weights in -128..=127, luma offsets in
    // -WpOffsetHalfRangeY..WpOffsetHalfRangeY, delta chroma offsets in
    // -4 * WpOffsetHalfRangeC..4 * WpOffsetHalfRangeC.
    let half_y: i64 = if wt.high_precision_offsets {
        1 << (sps.bit_depth_luma - 1)
    } else {
        128
    };
    let check = |v: i32, half: i64| {
        if (-half..half).contains(&(v as i64)) {
            Ok(v)
        } else {
            Err(DecodeError::InvalidSyntax(
                "pred_weight_table value out of range",
            ))
        }
    };

    let luma_denom = 1i16 << wt.luma_log2_weight_denom;
    let chroma_denom = 1i16 << wt.chroma_log2_weight_denom;

    // L0 weights.
    let mut luma_weight_l0_flag: Vec<bool> = Vec::with_capacity(nb_ref_l0 as usize);
    for _ in 0..nb_ref_l0 {
        luma_weight_l0_flag.push(r.read_bit()? == 1);
    }
    let mut chroma_weight_l0_flag: Vec<bool> = Vec::with_capacity(nb_ref_l0 as usize);
    if chroma_format_idc != 0 {
        for _ in 0..nb_ref_l0 {
            chroma_weight_l0_flag.push(r.read_bit()? == 1);
        }
    }
    for i in 0..nb_ref_l0 as usize {
        if luma_weight_l0_flag[i] {
            let delta = check(r.read_se()?, 128)? as i16;
            wt.luma_weight_l0[i] = luma_denom.saturating_add(delta);
            wt.luma_offset_l0[i] = check(r.read_se()?, half_y)? as i16;
        } else {
            wt.luma_weight_l0[i] = luma_denom;
            wt.luma_offset_l0[i] = 0;
        }
        if chroma_format_idc != 0 && i < chroma_weight_l0_flag.len() && chroma_weight_l0_flag[i] {
            for j in 0..2 {
                let delta_w = check(r.read_se()?, 128)? as i16;
                let delta_o = check(r.read_se()?, 4 * half_c)?;
                wt.chroma_weight_l0[i][j] = chroma_denom.saturating_add(delta_w);
                // Spec equation 7-59: effective offset includes the shift-back.
                // Use i64 to avoid overflow from malformed read_se() values.
                let w64 = wt.chroma_weight_l0[i][j] as i64;
                let off = (delta_o as i64 - ((half_c * w64) >> wt.chroma_log2_weight_denom)
                    + half_c)
                    .clamp(-half_c, half_c - 1);
                wt.chroma_offset_l0[i][j] = off as i16;
            }
        } else {
            wt.chroma_weight_l0[i] = [chroma_denom, chroma_denom];
            wt.chroma_offset_l0[i] = [0, 0];
        }
    }
    // L1 weights (B-slice only).
    if slice_type == SliceType::B {
        let mut luma_weight_l1_flag: Vec<bool> = Vec::with_capacity(nb_ref_l1 as usize);
        for _ in 0..nb_ref_l1 {
            luma_weight_l1_flag.push(r.read_bit()? == 1);
        }
        let mut chroma_weight_l1_flag: Vec<bool> = Vec::with_capacity(nb_ref_l1 as usize);
        if chroma_format_idc != 0 {
            for _ in 0..nb_ref_l1 {
                chroma_weight_l1_flag.push(r.read_bit()? == 1);
            }
        }
        for i in 0..nb_ref_l1 as usize {
            if luma_weight_l1_flag[i] {
                let delta = check(r.read_se()?, 128)? as i16;
                wt.luma_weight_l1[i] = luma_denom.saturating_add(delta);
                wt.luma_offset_l1[i] = check(r.read_se()?, half_y)? as i16;
            } else {
                wt.luma_weight_l1[i] = luma_denom;
                wt.luma_offset_l1[i] = 0;
            }
            if chroma_format_idc != 0 && i < chroma_weight_l1_flag.len() && chroma_weight_l1_flag[i]
            {
                for j in 0..2 {
                    let delta_w = check(r.read_se()?, 128)? as i16;
                    let delta_o = check(r.read_se()?, 4 * half_c)?;
                    wt.chroma_weight_l1[i][j] = chroma_denom.saturating_add(delta_w);
                    // Use i64 to avoid overflow from malformed read_se() values.
                    let w64 = wt.chroma_weight_l1[i][j] as i64;
                    let off = (delta_o as i64 - ((half_c * w64) >> wt.chroma_log2_weight_denom)
                        + half_c)
                        .clamp(-half_c, half_c - 1);
                    wt.chroma_offset_l1[i][j] = off as i16;
                }
            } else {
                wt.chroma_weight_l1[i] = [chroma_denom, chroma_denom];
                wt.chroma_offset_l1[i] = [0, 0];
            }
        }
    }
    Ok(wt)
}

/// Helper for `byte_alignment()` since the bitstream reader does not expose
/// "is at byte boundary" directly.
fn at_byte_boundary(r: &BitstreamReader) -> bool {
    let (_, bit_offset) = r.position();
    bit_offset == 0
}

/// `ceil(log2(n))` per spec convention (spec 5.2). Returns 0 for n <= 1.
fn ceil_log2(n: u32) -> u32 {
    if n <= 1 {
        0
    } else {
        32 - (n - 1).leading_zeros()
    }
}
