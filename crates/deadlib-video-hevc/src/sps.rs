//! HEVC Sequence Parameter Set parsing (spec 7.3.2.2, semantics 7.4.3.2).
//!
//! For Phase 1 we extract the fields needed by the rest of the decoder:
//! picture dimensions, CTU/CU/TU size derivation, bit depth, POC LSB width,
//! and the in-loop filter / RPS / temporal MVP enable flags. Anything more
//! exotic (scaling lists, ST-RPS entries, VUI parameters, extensions) is
//! either rejected or skipped.

use crate::bitstream::BitstreamReader;
use crate::error::DecodeError;
use crate::profile_tier_level::{ProfileTierLevel, parse_profile_tier_level};
use crate::scaling_list::{ScalingList, parse_scaling_list_data};

/// A parsed short-term reference picture set (spec 7.4.7.2).
///
/// Each entry references a previous decoded picture by **POC offset relative
/// to the current picture**. `delta_poc_s0` holds the negative offsets
/// (pictures before the current one) sorted in *decreasing* order of
/// magnitude (closest first, i.e. the smallest magnitude entry first, since
/// the deltas are negative). `delta_poc_s1` holds the positive offsets
/// (pictures after the current one) sorted in *increasing* order. Matches
/// the output of FFmpeg's `ff_hevc_decode_short_term_rps` after the sort
/// passes.
///
/// This is Phase 3d-1 bitstream plumbing — the fields are parsed and stored
/// but no actual inter decoding uses them yet.
#[derive(Debug, Clone, Default)]
pub struct ShortTermRps {
    /// Number of negative (pre-current) references.
    pub num_negative_pics: usize,
    /// Number of positive (post-current) references.
    pub num_positive_pics: usize,
    /// Signed POC delta for each negative reference (all entries < 0).
    /// Length `num_negative_pics`, sorted closest-first.
    pub delta_poc_s0: Vec<i32>,
    /// Signed POC delta for each positive reference (all entries > 0).
    /// Length `num_positive_pics`, sorted closest-first.
    pub delta_poc_s1: Vec<i32>,
    /// `used_by_curr_pic_s0_flag[i]` — if true, the matching ST-ref is
    /// "used by the current picture" (goes into RefPicSetStCurrBefore).
    pub used_by_curr_pic_s0_flag: Vec<bool>,
    /// `used_by_curr_pic_s1_flag[i]` — if true, the matching ST-ref is
    /// "used by the current picture" (goes into RefPicSetStCurrAfter).
    pub used_by_curr_pic_s1_flag: Vec<bool>,
}

impl ShortTermRps {
    /// Total number of delta POCs (`NumDeltaPocs = NumNegativePics + NumPositivePics`).
    pub fn num_delta_pocs(&self) -> usize {
        self.num_negative_pics + self.num_positive_pics
    }
}

/// Maximum allowed number of ST-RPSs in an SPS (spec annex A limit).
pub const MAX_SHORT_TERM_REF_PIC_SETS: usize = 64;

/// Maximum allowed number of LT reference pictures in an SPS.
pub const MAX_LONG_TERM_REF_PICS_SPS: usize = 32;

/// Parse a `st_ref_pic_set(stRpsIdx)` structure (spec 7.3.6.2).
///
/// `prev_rps_sets` are the ST-RPSs with indices `< stRpsIdx` that are
/// already parsed. They're needed for the "inter ref pic set prediction"
/// path (`inter_ref_pic_set_prediction_flag = 1`), which derives the
/// current RPS from a previously parsed one.
///
/// `num_short_term_ref_pic_sets` is the total number of ST-RPSs in the SPS.
/// For an ST-RPS parsed in the slice header (where this helper reuses the
/// same logic), `stRpsIdx = num_short_term_ref_pic_sets`, i.e. the slice
/// header RPS references the RPSs in `prev_rps_sets` by `delta_idx` too.
pub fn parse_st_ref_pic_set(
    r: &mut BitstreamReader,
    st_rps_idx: usize,
    num_short_term_ref_pic_sets: usize,
    prev_rps_sets: &[ShortTermRps],
) -> Result<ShortTermRps, DecodeError> {
    // `inter_ref_pic_set_prediction_flag` is only signaled when stRpsIdx > 0.
    // When stRpsIdx == 0 there is no previous RPS to predict from.
    let inter_ref_pic_set_prediction_flag = if st_rps_idx != 0 {
        r.read_bit()? == 1
    } else {
        false
    };

    if inter_ref_pic_set_prediction_flag {
        // Resolve the reference RPS index. Per spec 7.4.7.2:
        //   if (stRpsIdx == num_short_term_ref_pic_sets)
        //       delta_idx_minus1 = get_ue()
        //   else
        //       delta_idx_minus1 = 0 (inferred)
        //   RefRpsIdx = stRpsIdx - (delta_idx_minus1 + 1)
        let delta_idx_minus1 = if st_rps_idx == num_short_term_ref_pic_sets {
            r.read_ue()?
        } else {
            0
        };
        let delta_idx = (delta_idx_minus1 + 1) as usize;
        if delta_idx > st_rps_idx {
            return Err(DecodeError::InvalidSyntax(
                "st_ref_pic_set: delta_idx_minus1 too large",
            ));
        }
        let ref_rps_idx = st_rps_idx - delta_idx;
        // When parsing inside the SPS (st_rps_idx < num_short_term_ref_pic_sets),
        // we prefix-index prev_rps_sets by st_rps_idx. When parsing in the
        // slice header (st_rps_idx == num_short_term_ref_pic_sets), the full
        // SPS RPS array is the "prev" set; in both cases `prev_rps_sets[ref_rps_idx]`
        // is the correct lookup.
        let ref_rps = prev_rps_sets
            .get(ref_rps_idx)
            .ok_or(DecodeError::InvalidSyntax(
                "st_ref_pic_set: ref_rps_idx out of range",
            ))?
            .clone();

        let delta_rps_sign = r.read_bit()? == 1;
        let abs_delta_rps_minus1 = r.read_ue()?;
        if abs_delta_rps_minus1 >= 32768 {
            return Err(DecodeError::InvalidSyntax(
                "st_ref_pic_set: abs_delta_rps_minus1 out of range",
            ));
        }
        let delta_rps: i32 = if delta_rps_sign {
            -((abs_delta_rps_minus1 + 1) as i32)
        } else {
            (abs_delta_rps_minus1 + 1) as i32
        };

        // Read used_by_curr_pic_flag[j] and use_delta_flag[j] for
        // j = 0..=ref_rps.num_delta_pocs(). Note the <= — the loop is one
        // iteration longer than the number of delta POCs, to cover the new
        // "delta_rps" term added to the current picture.
        let ref_num_delta_pocs = ref_rps.num_delta_pocs();
        let mut used_by_curr_pic_flag = Vec::with_capacity(ref_num_delta_pocs + 1);
        let mut use_delta_flag = Vec::with_capacity(ref_num_delta_pocs + 1);
        for _ in 0..=ref_num_delta_pocs {
            let used = r.read_bit()? == 1;
            used_by_curr_pic_flag.push(used);
            if !used {
                use_delta_flag.push(r.read_bit()? == 1);
            } else {
                use_delta_flag.push(true);
            }
        }

        // Spec equations 7-65, 7-66, 7-67, 7-68: derive deltaPocS0/1[] and
        // usedByCurrPicS0/1[] for the current RPS. Ported from FFmpeg
        // `ff_hevc_decode_short_term_rps` (ps.c ~lines 103-190).
        //
        // We walk the reference RPS's negative pictures in reverse order,
        // the added delta_rps entry, then the reference RPS's positive
        // pictures in forward order; each entry that's either used or
        // includes a use_delta_flag becomes a delta POC in the new RPS.
        let mut delta_poc_s0: Vec<i32> = Vec::new();
        let mut delta_poc_s1: Vec<i32> = Vec::new();
        let mut used_s0: Vec<bool> = Vec::new();
        let mut used_s1: Vec<bool> = Vec::new();

        // Negative pictures: for j = num_negative_pics - 1 .. 0, the
        // reference RPS's negative delta[j], plus delta_rps. If the result
        // is still < 0 (and used_by_curr OR use_delta was set), it's a
        // negative picture of the new RPS.
        let ref_num_neg = ref_rps.num_negative_pics;
        let ref_num_pos = ref_rps.num_positive_pics;

        // Reverse walk over ref_rps negative pictures, then delta_rps on its own.
        // Using spec indexing: j corresponds to ref_rps.delta_poc_s1[j_pos] for positive,
        // but we actually iterate ref_rps.delta_poc concatenated. Let's follow the
        // FFmpeg approach more directly: FFmpeg builds a flat delta_poc list
        // sorted ascending. Re-order: ref_rps has delta_poc_s0 with magnitudes
        // sorted closest-first (decreasing abs), delta_poc_s1 closest-first
        // (increasing).
        //
        // Build a flat list [all s0 in ascending order, all s1 in ascending
        // order] to mirror FFmpeg's layout.
        let mut ref_flat: Vec<i32> = Vec::with_capacity(ref_num_delta_pocs);
        // s0 is stored closest-first (e.g. [-1, -3, -5]); we want ascending
        // (most-negative-first): [-5, -3, -1]. Reverse it.
        for d in ref_rps.delta_poc_s0.iter().rev() {
            ref_flat.push(*d);
        }
        for d in ref_rps.delta_poc_s1.iter() {
            ref_flat.push(*d);
        }
        // The extra iteration (j == ref_num_delta_pocs) corresponds to delta_rps
        // alone; it's inserted between s0 and s1 (before positives).
        //
        // Spec 7-65 / 7-66 / 7-67 / 7-68:
        //   for (j = num_positive_pics_ref - 1; j >= 0; j--)
        //       dPoc = deltaPocS1Ref[j] + deltaRps
        //       if (dPoc < 0 && use_delta_flag[num_neg_ref + j])
        //           DeltaPocS0[i++] = dPoc
        //           UsedByCurrPicS0[i - 1] = used_by_curr_pic_flag[num_neg_ref + j]
        //   if (deltaRps < 0 && use_delta_flag[num_delta_pocs_ref])
        //       DeltaPocS0[i++] = deltaRps
        //       UsedByCurrPicS0[i - 1] = used_by_curr_pic_flag[num_delta_pocs_ref]
        //   for (j = 0; j < num_neg_ref; j++)
        //       dPoc = deltaPocS0Ref[j] + deltaRps
        //       if (dPoc < 0 && use_delta_flag[j])
        //           DeltaPocS0[i++] = dPoc
        //           UsedByCurrPicS0[i - 1] = used_by_curr_pic_flag[j]
        //
        // And symmetrically for DeltaPocS1.
        //
        // Note: ref_rps.delta_poc_s0 is indexed closest-first (j=0 is the
        // closest negative, i.e. smallest |delta|). "deltaPocS0Ref[j]" in
        // the spec uses the same convention.

        // DeltaPocS0 (negative refs of the new RPS):
        for j in (0..ref_num_pos).rev() {
            let d_poc = ref_rps.delta_poc_s1[j] + delta_rps;
            if d_poc < 0 && use_delta_flag[ref_num_neg + j] {
                delta_poc_s0.push(d_poc);
                used_s0.push(used_by_curr_pic_flag[ref_num_neg + j]);
            }
        }
        if delta_rps < 0 && use_delta_flag[ref_num_delta_pocs] {
            delta_poc_s0.push(delta_rps);
            used_s0.push(used_by_curr_pic_flag[ref_num_delta_pocs]);
        }
        for j in 0..ref_num_neg {
            let d_poc = ref_rps.delta_poc_s0[j] + delta_rps;
            if d_poc < 0 && use_delta_flag[j] {
                delta_poc_s0.push(d_poc);
                used_s0.push(used_by_curr_pic_flag[j]);
            }
        }

        // DeltaPocS1 (positive refs of the new RPS):
        for j in (0..ref_num_neg).rev() {
            let d_poc = ref_rps.delta_poc_s0[j] + delta_rps;
            if d_poc > 0 && use_delta_flag[j] {
                delta_poc_s1.push(d_poc);
                used_s1.push(used_by_curr_pic_flag[j]);
            }
        }
        if delta_rps > 0 && use_delta_flag[ref_num_delta_pocs] {
            delta_poc_s1.push(delta_rps);
            used_s1.push(used_by_curr_pic_flag[ref_num_delta_pocs]);
        }
        for j in 0..ref_num_pos {
            let d_poc = ref_rps.delta_poc_s1[j] + delta_rps;
            if d_poc > 0 && use_delta_flag[ref_num_neg + j] {
                delta_poc_s1.push(d_poc);
                used_s1.push(used_by_curr_pic_flag[ref_num_neg + j]);
            }
        }

        // `delta_poc_s0` we built above is already in "closest-first"
        // order thanks to the walk direction. Same for `delta_poc_s1`.

        // A predicted set can hold one more entry than the set it predicts
        // from, so chained predictions could otherwise grow without bound.
        // Apply the same limit as the explicit path (spec 7.4.8 bounds both
        // counts by sps_max_dec_pic_buffering_minus1, at most 15).
        if delta_poc_s0.len() > 16 || delta_poc_s1.len() > 16 {
            return Err(DecodeError::InvalidSyntax(
                "st_ref_pic_set: too many reference pictures",
            ));
        }

        Ok(ShortTermRps {
            num_negative_pics: delta_poc_s0.len(),
            num_positive_pics: delta_poc_s1.len(),
            delta_poc_s0,
            delta_poc_s1,
            used_by_curr_pic_s0_flag: used_s0,
            used_by_curr_pic_s1_flag: used_s1,
        })
    } else {
        let num_negative_pics = r.read_ue()? as usize;
        let num_positive_pics = r.read_ue()? as usize;
        if num_negative_pics > 16 || num_positive_pics > 16 {
            return Err(DecodeError::InvalidSyntax(
                "st_ref_pic_set: too many reference pictures",
            ));
        }

        let mut delta_poc_s0 = Vec::with_capacity(num_negative_pics);
        let mut used_by_curr_pic_s0_flag = Vec::with_capacity(num_negative_pics);
        let mut prev: i32 = 0;
        for _ in 0..num_negative_pics {
            let delta_poc_s0_minus1 = r.read_ue()?;
            if delta_poc_s0_minus1 >= 32768 {
                return Err(DecodeError::InvalidSyntax(
                    "st_ref_pic_set: delta_poc_s0_minus1 out of range",
                ));
            }
            prev -= (delta_poc_s0_minus1 + 1) as i32;
            delta_poc_s0.push(prev);
            used_by_curr_pic_s0_flag.push(r.read_bit()? == 1);
        }

        let mut delta_poc_s1 = Vec::with_capacity(num_positive_pics);
        let mut used_by_curr_pic_s1_flag = Vec::with_capacity(num_positive_pics);
        let mut prev: i32 = 0;
        for _ in 0..num_positive_pics {
            let delta_poc_s1_minus1 = r.read_ue()?;
            if delta_poc_s1_minus1 >= 32768 {
                return Err(DecodeError::InvalidSyntax(
                    "st_ref_pic_set: delta_poc_s1_minus1 out of range",
                ));
            }
            prev += (delta_poc_s1_minus1 + 1) as i32;
            delta_poc_s1.push(prev);
            used_by_curr_pic_s1_flag.push(r.read_bit()? == 1);
        }

        Ok(ShortTermRps {
            num_negative_pics,
            num_positive_pics,
            delta_poc_s0,
            delta_poc_s1,
            used_by_curr_pic_s0_flag,
            used_by_curr_pic_s1_flag,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Sps {
    pub sps_video_parameter_set_id: u8,
    pub sps_max_sub_layers_minus1: u8,
    pub sps_temporal_id_nesting_flag: bool,
    pub profile_tier_level: ProfileTierLevel,
    pub sps_seq_parameter_set_id: u32,
    pub chroma_format_idc: u32,
    /// When `chroma_format_idc == 3` and this is set, the three colour planes
    /// are coded as separate monochrome pictures (`ChromaArrayType = 0`).
    pub separate_colour_plane_flag: bool,
    pub pic_width_in_luma_samples: u32,
    pub pic_height_in_luma_samples: u32,
    /// Conformance window crop offsets (in chroma sample units for 4:2:0).
    pub conf_win_left_offset: u32,
    pub conf_win_right_offset: u32,
    pub conf_win_top_offset: u32,
    pub conf_win_bottom_offset: u32,
    pub bit_depth_luma: u8,
    pub bit_depth_chroma: u8,
    pub log2_max_pic_order_cnt_lsb: u8,

    // CU/CTU derivation per spec 7.4.3.2.1
    pub min_cb_log2_size_y: u8,
    pub ctb_log2_size_y: u8,
    pub ctb_size_y: u32,
    pub min_tb_log2_size_y: u8,
    pub max_tb_log2_size_y: u8,
    pub max_transform_hierarchy_depth_inter: u32,
    pub max_transform_hierarchy_depth_intra: u32,

    pub scaling_list_enabled_flag: bool,
    /// The active scaling list. Present when `scaling_list_enabled_flag` is true.
    /// Contains default values when `sps_scaling_list_data_present_flag` is false,
    /// or explicitly parsed values when true.
    pub scaling_list: Option<ScalingList>,
    pub amp_enabled_flag: bool,
    pub sample_adaptive_offset_enabled_flag: bool,
    pub pcm_enabled_flag: bool,
    /// Bit depth of luma PCM samples (`pcm_sample_bit_depth_luma_minus1 + 1`).
    /// Only meaningful when `pcm_enabled_flag` is true; defaults to
    /// `bit_depth_luma` otherwise.
    pub pcm_sample_bit_depth_luma: u8,
    /// Bit depth of chroma PCM samples.
    pub pcm_sample_bit_depth_chroma: u8,
    /// `Log2MinIpcmCbSizeY` (spec eq. 7-35).
    pub log2_min_pcm_cb_size: u8,
    /// `Log2MaxIpcmCbSizeY` = `log2_min_pcm_cb_size + log2_diff_max_min_pcm_luma_coding_block_size`.
    pub log2_max_pcm_cb_size: u8,
    /// When set, deblocking is disabled across the boundaries of PCM blocks
    /// in this SPS. We don't have deblocking yet so we just store this.
    pub pcm_loop_filter_disabled_flag: bool,
    pub num_short_term_ref_pic_sets: u32,
    /// Phase 3d-1: parsed SPS short-term reference picture sets. Length
    /// `num_short_term_ref_pic_sets`. Used by slice headers that select an
    /// RPS by index via `short_term_ref_pic_set_sps_flag`.
    pub st_ref_pic_sets: Vec<ShortTermRps>,
    pub long_term_ref_pics_present_flag: bool,
    /// Phase 3d-1: SPS long-term reference picture info
    /// (`num_long_term_ref_pics_sps`, `lt_ref_pic_poc_lsb_sps[]`,
    /// `used_by_curr_pic_lt_sps_flag[]`). Empty when
    /// `long_term_ref_pics_present_flag == false`.
    pub num_long_term_ref_pics_sps: u32,
    pub lt_ref_pic_poc_lsb_sps: Vec<u32>,
    pub used_by_curr_pic_lt_sps_flag: Vec<bool>,
    pub sps_temporal_mvp_enabled_flag: bool,
    pub strong_intra_smoothing_enabled_flag: bool,
    /// Phase 3d-1: DPB sizing fields parsed from the "sub-layer ordering
    /// info" in the SPS. Stored at the topmost sublayer
    /// (`sps_max_sub_layers_minus1`). Used by `DecodedPictureBuffer` to
    /// decide when to bump pictures out to the display.
    pub sps_max_dec_pic_buffering_minus1: u32,
    pub sps_max_num_reorder_pics: u32,
    pub sps_max_latency_increase_plus1: u32,
    /// `sps_range_extension()` flags (spec 7.3.2.2.2); all false when absent.
    pub range: SpsRangeExtension,
}

/// Coding-tool flags of `sps_range_extension()` (spec 7.4.3.2.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpsRangeExtension {
    pub transform_skip_rotation_enabled_flag: bool,
    pub transform_skip_context_enabled_flag: bool,
    pub implicit_rdpcm_enabled_flag: bool,
    pub explicit_rdpcm_enabled_flag: bool,
    pub extended_precision_processing_flag: bool,
    pub intra_smoothing_disabled_flag: bool,
    pub high_precision_offsets_enabled_flag: bool,
    pub persistent_rice_adaptation_enabled_flag: bool,
    pub cabac_bypass_alignment_enabled_flag: bool,
}

impl Sps {
    /// Whether samples need 16-bit storage: luma or chroma deeper than 8
    /// bits (the two bit depths may differ).
    pub fn needs_u16_samples(&self) -> bool {
        self.bit_depth_luma.max(self.bit_depth_chroma) > 8
    }

    /// `ChromaArrayType` (spec 7.4.3.2.1): equals `chroma_format_idc` unless
    /// the 4:4:4 planes are coded separately, in which case it is 0.
    pub fn chroma_array_type(&self) -> u32 {
        if self.separate_colour_plane_flag {
            0
        } else {
            self.chroma_format_idc
        }
    }

    /// `SubWidthC` (spec table 6-1): horizontal chroma subsampling factor.
    /// 4:2:0 and 4:2:2 → 2; 4:4:4 and monochrome → 1.
    pub fn sub_width_c(&self) -> u32 {
        match self.chroma_array_type() {
            1 | 2 => 2,
            _ => 1,
        }
    }

    /// `SubHeightC` (spec table 6-1): vertical chroma subsampling factor.
    /// 4:2:0 → 2; 4:2:2, 4:4:4, monochrome → 1.
    pub fn sub_height_c(&self) -> u32 {
        match self.chroma_array_type() {
            1 => 2,
            _ => 1,
        }
    }

    /// `PicWidthInCtbsY` (spec eq. 7-15).
    pub fn pic_width_in_ctbs_y(&self) -> u32 {
        self.pic_width_in_luma_samples.div_ceil(self.ctb_size_y)
    }

    /// `PicHeightInCtbsY` (spec eq. 7-17).
    pub fn pic_height_in_ctbs_y(&self) -> u32 {
        self.pic_height_in_luma_samples.div_ceil(self.ctb_size_y)
    }

    /// Cropped output width in luma samples (after conformance window).
    /// For 4:2:0, `SubWidthC = 2`. Saturates to 0 on malformed input where
    /// the conformance window exceeds the picture dimensions.
    pub fn cropped_width(&self) -> u32 {
        let sub_width_c: u32 = if self.chroma_format_idc == 1 { 2 } else { 1 };
        let crop = sub_width_c.saturating_mul(
            self.conf_win_left_offset
                .saturating_add(self.conf_win_right_offset),
        );
        self.pic_width_in_luma_samples.saturating_sub(crop)
    }

    /// Cropped output height in luma samples (after conformance window).
    /// For 4:2:0, `SubHeightC = 2`. Saturates to 0 on malformed input.
    pub fn cropped_height(&self) -> u32 {
        let sub_height_c: u32 = if self.chroma_format_idc == 1 { 2 } else { 1 };
        let crop = sub_height_c.saturating_mul(
            self.conf_win_top_offset
                .saturating_add(self.conf_win_bottom_offset),
        );
        self.pic_height_in_luma_samples.saturating_sub(crop)
    }
}

/// Reject a conformance window that does not fit inside the coded picture.
/// Offsets are in chroma sample units, so they scale by `SubWidthC` /
/// `SubHeightC` (2 for 4:2:0, 1 for 4:4:4) to reach luma samples.
#[allow(clippy::too_many_arguments)]
fn validate_conformance_window(
    width: u32,
    height: u32,
    sub_width_c: u32,
    sub_height_c: u32,
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
) -> Result<(), DecodeError> {
    let cropped_x = left
        .checked_add(right)
        .and_then(|value| value.checked_mul(sub_width_c))
        .ok_or(DecodeError::InvalidSyntax(
            "conformance window width overflow",
        ))?;
    let cropped_y = top
        .checked_add(bottom)
        .and_then(|value| value.checked_mul(sub_height_c))
        .ok_or(DecodeError::InvalidSyntax(
            "conformance window height overflow",
        ))?;
    // 7.4.3.2.1: the window keeps at least one sample in each direction.
    if cropped_x >= width || cropped_y >= height {
        return Err(DecodeError::InvalidSyntax(
            "conformance window exceeds picture dimensions",
        ));
    }
    Ok(())
}

/// Cross-field transform block size constraints (spec 7.4.3.2.1), which
/// FFmpeg also enforces: MinTbLog2SizeY < MinCbLog2SizeY and
/// MaxTbLog2SizeY <= Min(CtbLog2SizeY, 5). A min TB larger than the CTB
/// leaves per-min-TB tables with zero columns for small pictures.
fn validate_transform_block_sizes(
    min_cb_log2: u8,
    ctb_log2: u8,
    min_tb_log2: u8,
    max_tb_log2: u8,
) -> Result<(), DecodeError> {
    if min_tb_log2 >= min_cb_log2 {
        return Err(DecodeError::InvalidSyntax(
            "MinTbLog2SizeY must be less than MinCbLog2SizeY",
        ));
    }
    if max_tb_log2 > ctb_log2.min(5) {
        return Err(DecodeError::InvalidSyntax(
            "MaxTbLog2SizeY exceeds Min(CtbLog2SizeY, 5)",
        ));
    }
    Ok(())
}

/// `max_transform_hierarchy_depth_inter` / `_intra` must each lie in
/// 0..=CtbLog2SizeY - MinTbLog2SizeY (spec 7.4.3.2.1, enforced by FFmpeg).
fn validate_transform_hierarchy_depths(
    ctb_log2: u8,
    min_tb_log2: u8,
    depth_inter: u32,
    depth_intra: u32,
) -> Result<(), DecodeError> {
    let max_depth = u32::from(ctb_log2.saturating_sub(min_tb_log2));
    if depth_inter > max_depth || depth_intra > max_depth {
        return Err(DecodeError::InvalidSyntax(
            "max_transform_hierarchy_depth out of range",
        ));
    }
    Ok(())
}

/// `sps_seq_parameter_set_id` of an SPS, for remembering a rejected SPS
/// under its ID.
pub fn peek_sps_id(rbsp: &[u8]) -> Option<u32> {
    let mut r = BitstreamReader::new(rbsp);
    r.read_bits(4).ok()?;
    let max_sub_layers_minus1 = r.read_bits(3).ok()? as u8;
    if max_sub_layers_minus1 > 6 {
        return None;
    }
    r.read_bit().ok()?;
    parse_profile_tier_level(&mut r, max_sub_layers_minus1).ok()?;
    r.read_ue().ok()
}

/// Skip `vui_parameters()` (spec E.2.1) and read `sps_range_extension()`.
/// The screen content coding extension is rejected as `Unsupported`.
fn parse_vui_and_extensions(
    r: &mut BitstreamReader,
    max_sub_layers_minus1: u8,
) -> Result<SpsRangeExtension, DecodeError> {
    if r.read_bit()? == 1 {
        skip_vui(r, max_sub_layers_minus1)?;
    }
    let mut ext = SpsRangeExtension::default();
    if r.read_bit()? == 0 {
        return Ok(ext); // sps_extension_present_flag
    }
    let range = r.read_bit()? == 1;
    let _multilayer = r.read_bit()?;
    let _3d = r.read_bit()?;
    let scc = r.read_bit()? == 1;
    let _extension_4bits = r.read_bits(4)?;
    if range {
        // sps_range_extension() (spec 7.3.2.2.2), in syntax order.
        let mut flag = || -> Result<bool, DecodeError> { Ok(r.read_bit()? == 1) };
        ext.transform_skip_rotation_enabled_flag = flag()?;
        ext.transform_skip_context_enabled_flag = flag()?;
        ext.implicit_rdpcm_enabled_flag = flag()?;
        ext.explicit_rdpcm_enabled_flag = flag()?;
        ext.extended_precision_processing_flag = flag()?;
        ext.intra_smoothing_disabled_flag = flag()?;
        ext.high_precision_offsets_enabled_flag = flag()?;
        ext.persistent_rice_adaptation_enabled_flag = flag()?;
        ext.cabac_bypass_alignment_enabled_flag = flag()?;
    }
    // The multilayer and 3D extensions only carry constraints for layers
    // above the base layer, which is all this decoder decodes; their data
    // follows the range extension and needs no parsing.
    if scc {
        return Err(DecodeError::Unsupported("screen content coding extension"));
    }
    Ok(ext)
}

/// Skip `vui_parameters()` (spec E.2.1).
fn skip_vui(r: &mut BitstreamReader, max_sub_layers_minus1: u8) -> Result<(), DecodeError> {
    if r.read_bit()? == 1 {
        // aspect_ratio_idc; EXTENDED_SAR adds sar_width / sar_height.
        if r.read_bits(8)? == 255 {
            r.read_bits(16)?;
            r.read_bits(16)?;
        }
    }
    if r.read_bit()? == 1 {
        r.read_bit()?; // overscan_appropriate_flag
    }
    if r.read_bit()? == 1 {
        // video_format, video_full_range_flag
        r.read_bits(4)?;
        if r.read_bit()? == 1 {
            // colour_primaries, transfer_characteristics, matrix_coeffs
            r.read_bits(24)?;
        }
    }
    if r.read_bit()? == 1 {
        r.read_ue()?; // chroma_sample_loc_type_top_field
        r.read_ue()?; // chroma_sample_loc_type_bottom_field
    }
    // neutral_chroma_indication_flag, field_seq_flag,
    // frame_field_info_present_flag
    r.read_bits(3)?;
    if r.read_bit()? == 1 {
        for _ in 0..4 {
            r.read_ue()?; // default display window offsets
        }
    }
    if r.read_bit()? == 1 {
        // vui_num_units_in_tick, vui_time_scale
        r.read_bits(32)?;
        r.read_bits(32)?;
        if r.read_bit()? == 1 {
            r.read_ue()?; // vui_num_ticks_poc_diff_one_minus1
        }
        if r.read_bit()? == 1 {
            skip_hrd_parameters(r, true, max_sub_layers_minus1)?;
        }
    }
    if r.read_bit()? == 1 {
        // tiles_fixed_structure_flag, motion_vectors_over_pic_boundaries_flag,
        // restricted_ref_pic_lists_flag
        r.read_bits(3)?;
        for _ in 0..5 {
            // min_spatial_segmentation_idc, max_bytes_per_pic_denom,
            // max_bits_per_min_cu_denom, log2_max_mv_length_horizontal,
            // log2_max_mv_length_vertical
            r.read_ue()?;
        }
    }
    Ok(())
}

/// Skip `hrd_parameters()` (spec E.2.2).
fn skip_hrd_parameters(
    r: &mut BitstreamReader,
    common_inf_present: bool,
    max_sub_layers_minus1: u8,
) -> Result<(), DecodeError> {
    let (mut nal_hrd, mut vcl_hrd, mut sub_pic) = (false, false, false);
    if common_inf_present {
        nal_hrd = r.read_bit()? == 1;
        vcl_hrd = r.read_bit()? == 1;
        if nal_hrd || vcl_hrd {
            sub_pic = r.read_bit()? == 1;
            if sub_pic {
                // tick_divisor_minus2 (8), du_cpb_removal_delay_increment_
                // length_minus1 (5), sub_pic_cpb_params_in_pic_timing_sei_flag
                // (1), dpb_output_delay_du_length_minus1 (5)
                r.read_bits(19)?;
            }
            r.read_bits(8)?; // bit_rate_scale, cpb_size_scale
            if sub_pic {
                r.read_bits(4)?; // cpb_size_du_scale
            }
            // initial_cpb_removal_delay_length_minus1,
            // au_cpb_removal_delay_length_minus1, dpb_output_delay_length_minus1
            r.read_bits(15)?;
        }
    }
    for _ in 0..=max_sub_layers_minus1 {
        let fixed_general = r.read_bit()? == 1;
        let fixed_within_cvs = fixed_general || r.read_bit()? == 1;
        let mut low_delay = false;
        if fixed_within_cvs {
            r.read_ue()?; // elemental_duration_in_tc_minus1
        } else {
            low_delay = r.read_bit()? == 1;
        }
        let cpb_cnt = if low_delay { 1 } else { r.read_ue()? + 1 };
        if cpb_cnt > 32 {
            return Err(DecodeError::InvalidSyntax("cpb_cnt_minus1 out of range"));
        }
        for present in [nal_hrd, vcl_hrd] {
            if !present {
                continue;
            }
            // sub_layer_hrd_parameters()
            for _ in 0..cpb_cnt {
                r.read_ue()?; // bit_rate_value_minus1
                r.read_ue()?; // cpb_size_value_minus1
                if sub_pic {
                    r.read_ue()?; // cpb_size_du_value_minus1
                    r.read_ue()?; // bit_rate_du_value_minus1
                }
                r.read_bit()?; // cbr_flag
            }
        }
    }
    Ok(())
}

pub fn parse_sps(rbsp: &[u8]) -> Result<Sps, DecodeError> {
    let mut r = BitstreamReader::new(rbsp);

    let sps_video_parameter_set_id = r.read_bits(4)? as u8;
    let sps_max_sub_layers_minus1 = r.read_bits(3)? as u8;
    if sps_max_sub_layers_minus1 > 6 {
        return Err(DecodeError::InvalidSyntax(
            "sps_max_sub_layers_minus1 out of range",
        ));
    }
    let sps_temporal_id_nesting_flag = r.read_bit()? == 1;
    let ptl = parse_profile_tier_level(&mut r, sps_max_sub_layers_minus1)?;

    let sps_seq_parameter_set_id = r.read_ue()?;
    let chroma_format_idc = r.read_ue()?;
    if chroma_format_idc > 3 {
        return Err(DecodeError::InvalidSyntax("chroma_format_idc out of range"));
    }
    let mut separate_colour_plane_flag = false;
    if chroma_format_idc == 3 {
        separate_colour_plane_flag = r.read_bit()? == 1;
    }
    // Supported ChromaArrayTypes: 4:2:0 (1) and 4:4:4 (3). 4:2:2 (2) and
    // monochrome (0, including 4:4:4 with separate colour planes) are not yet
    // wired through the reconstruction pipeline. The SPS parse stays in sync
    // regardless. See the chroma-geometry helpers on `Sps`.
    let chroma_array_type = if separate_colour_plane_flag {
        0
    } else {
        chroma_format_idc
    };
    if chroma_array_type != 1 && chroma_array_type != 3 {
        return Err(DecodeError::Unsupported(
            "only 4:2:0 and 4:4:4 chroma formats supported",
        ));
    }

    let pic_width_in_luma_samples = r.read_ue()?;
    let pic_height_in_luma_samples = r.read_ue()?;
    // Sanity-check dimensions to prevent OOM on crafted bitstreams.
    // HEVC Level 6.2 max is 8192×4320; we allow up to 16384×16384 for
    // headroom. The pixel plane allocation is O(w*h) so unchecked values
    // from a malicious SPS could exhaust memory.
    if pic_width_in_luma_samples == 0
        || pic_height_in_luma_samples == 0
        || pic_width_in_luma_samples > 16384
        || pic_height_in_luma_samples > 16384
    {
        return Err(DecodeError::InvalidSyntax(
            "pic_width/height_in_luma_samples out of range (0 or >16384)",
        ));
    }

    let conformance_window_flag = r.read_bit()? == 1;
    let mut conf_win_left_offset = 0u32;
    let mut conf_win_right_offset = 0u32;
    let mut conf_win_top_offset = 0u32;
    let mut conf_win_bottom_offset = 0u32;
    if conformance_window_flag {
        conf_win_left_offset = r.read_ue()?;
        conf_win_right_offset = r.read_ue()?;
        conf_win_top_offset = r.read_ue()?;
        conf_win_bottom_offset = r.read_ue()?;
        // Supported ChromaArrayTypes are 1 (4:2:0, SubWidthC = SubHeightC = 2)
        // and 3 (4:4:4, both 1).
        let (sub_w, sub_h) = if chroma_array_type == 1 {
            (2, 2)
        } else {
            (1, 1)
        };
        validate_conformance_window(
            pic_width_in_luma_samples,
            pic_height_in_luma_samples,
            sub_w,
            sub_h,
            conf_win_left_offset,
            conf_win_right_offset,
            conf_win_top_offset,
            conf_win_bottom_offset,
        )?;
    }

    let bit_depth_luma_minus8 = r.read_ue()?;
    let bit_depth_chroma_minus8 = r.read_ue()?;
    // Up to 12 bits are implemented. Deeper samples need extended
    // precision arithmetic (motion compensation shifts of 14 - BitDepth
    // would go negative), which this decoder does not do.
    if bit_depth_luma_minus8 > 4 || bit_depth_chroma_minus8 > 4 {
        return Err(DecodeError::Unsupported("bit depth > 12"));
    }
    let bit_depth_luma = 8 + bit_depth_luma_minus8 as u8;
    let bit_depth_chroma = 8 + bit_depth_chroma_minus8 as u8;

    let log2_max_pic_order_cnt_lsb_minus4 = r.read_ue()?;
    if log2_max_pic_order_cnt_lsb_minus4 > 12 {
        return Err(DecodeError::InvalidSyntax(
            "log2_max_pic_order_cnt_lsb_minus4 out of range",
        ));
    }
    let log2_max_pic_order_cnt_lsb = (log2_max_pic_order_cnt_lsb_minus4 + 4) as u8;

    let sps_sub_layer_ordering_info_present_flag = r.read_bit()? == 1;
    let i_start = if sps_sub_layer_ordering_info_present_flag {
        0
    } else {
        sps_max_sub_layers_minus1 as usize
    };
    // Phase 3d-1: capture the topmost sub-layer's DPB sizing fields.
    // The spec says fields for sub-layers below `sps_max_sub_layers_minus1`
    // are only signaled when the present flag is set; otherwise all layers
    // inherit the topmost layer's values. For our single-sublayer fixtures
    // we simply capture whatever sublayer is actually signaled.
    let mut sps_max_dec_pic_buffering_minus1: u32 = 0;
    let mut sps_max_num_reorder_pics: u32 = 0;
    let mut sps_max_latency_increase_plus1: u32 = 0;
    for _ in i_start..=sps_max_sub_layers_minus1 as usize {
        sps_max_dec_pic_buffering_minus1 = r.read_ue()?;
        sps_max_num_reorder_pics = r.read_ue()?;
        sps_max_latency_increase_plus1 = r.read_ue()?;
    }
    // 7.4.3.2.1 / A.4.2: the DPB holds at most MaxDpbSize = 16 pictures,
    // and the reorder count is below its size. As FFmpeg, a reorder count
    // that is too large but possible enlarges the DPB instead.
    const MAX_DPB_SIZE: u32 = 16;
    if sps_max_dec_pic_buffering_minus1 >= MAX_DPB_SIZE || sps_max_num_reorder_pics >= MAX_DPB_SIZE
    {
        return Err(DecodeError::InvalidSyntax(
            "sps_max_dec_pic_buffering / num_reorder_pics out of range",
        ));
    }
    sps_max_dec_pic_buffering_minus1 =
        sps_max_dec_pic_buffering_minus1.max(sps_max_num_reorder_pics);

    let log2_min_luma_coding_block_size_minus3 = r.read_ue()?;
    let log2_diff_max_min_luma_coding_block_size = r.read_ue()?;
    // Validate before u8 cast to prevent overflow on malformed input.
    if log2_min_luma_coding_block_size_minus3 > 3 || log2_diff_max_min_luma_coding_block_size > 3 {
        return Err(DecodeError::InvalidSyntax(
            "log2_min_luma_coding_block_size or diff out of range",
        ));
    }
    let min_cb_log2_size_y = (log2_min_luma_coding_block_size_minus3 + 3) as u8;
    let ctb_log2_size_y = min_cb_log2_size_y + log2_diff_max_min_luma_coding_block_size as u8;
    if !(4..=6).contains(&ctb_log2_size_y) {
        // Spec restricts CtbLog2SizeY to 4..=6 (CTU size 16/32/64).
        return Err(DecodeError::InvalidSyntax("CtbLog2SizeY out of range"));
    }
    let ctb_size_y: u32 = 1 << ctb_log2_size_y;
    // Spec 7.4.3.2.1: the picture size is a multiple of MinCbSizeY (FFmpeg
    // rejects it too). The 4×4-grid tables (boundary strengths, motion
    // field) rely on it: a picture 1 sample wide has a grid 0 units wide.
    let min_cb_mask = (1u32 << min_cb_log2_size_y) - 1;
    if pic_width_in_luma_samples & min_cb_mask != 0 || pic_height_in_luma_samples & min_cb_mask != 0
    {
        return Err(DecodeError::InvalidSyntax(
            "pic_width/height_in_luma_samples not a multiple of MinCbSizeY",
        ));
    }

    let log2_min_luma_transform_block_size_minus2 = r.read_ue()?;
    let log2_diff_max_min_luma_transform_block_size = r.read_ue()?;
    // Spec: MinTbLog2SizeY ranges 2..5, diff ranges 0..3.
    if log2_min_luma_transform_block_size_minus2 > 3
        || log2_diff_max_min_luma_transform_block_size > 3
    {
        return Err(DecodeError::InvalidSyntax(
            "log2 transform block size params out of range",
        ));
    }
    let min_tb_log2_size_y = (log2_min_luma_transform_block_size_minus2 + 2) as u8;
    let max_tb_log2_size_y = min_tb_log2_size_y + log2_diff_max_min_luma_transform_block_size as u8;
    validate_transform_block_sizes(
        min_cb_log2_size_y,
        ctb_log2_size_y,
        min_tb_log2_size_y,
        max_tb_log2_size_y,
    )?;

    let max_transform_hierarchy_depth_inter = r.read_ue()?;
    let max_transform_hierarchy_depth_intra = r.read_ue()?;
    validate_transform_hierarchy_depths(
        ctb_log2_size_y,
        min_tb_log2_size_y,
        max_transform_hierarchy_depth_inter,
        max_transform_hierarchy_depth_intra,
    )?;

    let scaling_list_enabled_flag = r.read_bit()? == 1;
    let scaling_list = if scaling_list_enabled_flag {
        let mut sl = ScalingList::default_scaling_list();
        let sps_scaling_list_data_present_flag = r.read_bit()? == 1;
        if sps_scaling_list_data_present_flag {
            parse_scaling_list_data(&mut r, &mut sl)?;
        }
        Some(sl)
    } else {
        None
    };

    let amp_enabled_flag = r.read_bit()? == 1;
    let sample_adaptive_offset_enabled_flag = r.read_bit()? == 1;

    let pcm_enabled_flag = r.read_bit()? == 1;
    let (
        pcm_sample_bit_depth_luma,
        pcm_sample_bit_depth_chroma,
        log2_min_pcm_cb_size,
        log2_max_pcm_cb_size,
        pcm_loop_filter_disabled_flag,
    ) = if pcm_enabled_flag {
        // Spec 7.3.2.2 + 7.4.3.2.1.
        let pcm_bd_luma = (r.read_bits(4)? + 1) as u8;
        let pcm_bd_chroma = (r.read_bits(4)? + 1) as u8;
        if pcm_bd_luma > bit_depth_luma || pcm_bd_chroma > bit_depth_chroma {
            return Err(DecodeError::InvalidSyntax(
                "pcm_sample_bit_depth exceeds bit depth",
            ));
        }
        let log2_min_pcm_cb_size_minus3 = r.read_ue()?;
        let log2_diff_max_min_pcm = r.read_ue()?;
        if log2_min_pcm_cb_size_minus3 > 2 || log2_diff_max_min_pcm > 2 {
            return Err(DecodeError::InvalidSyntax(
                "pcm log2 coding block size params out of range",
            ));
        }
        let log2_min_pcm_cb_size = (log2_min_pcm_cb_size_minus3 + 3) as u8;
        let log2_max_pcm_cb_size = log2_min_pcm_cb_size + log2_diff_max_min_pcm as u8;
        let pcm_loop_filter_disabled = r.read_bit()? == 1;
        (
            pcm_bd_luma,
            pcm_bd_chroma,
            log2_min_pcm_cb_size,
            log2_max_pcm_cb_size,
            pcm_loop_filter_disabled,
        )
    } else {
        // Defaults when PCM is disabled. `log2_min_pcm_cb_size > log2_max_pcm_cb_size`
        // ensures the "in range" test in `decode_coding_unit` never fires.
        (bit_depth_luma, bit_depth_chroma, 8u8, 0u8, false)
    };

    let num_short_term_ref_pic_sets = r.read_ue()?;
    if num_short_term_ref_pic_sets as usize > MAX_SHORT_TERM_REF_PIC_SETS {
        return Err(DecodeError::InvalidSyntax(
            "num_short_term_ref_pic_sets out of range",
        ));
    }
    // Phase 3d-1: parse every SPS short-term RPS. Stored on the SPS for
    // later slice-header lookup via `short_term_ref_pic_set_sps_flag`.
    let mut st_ref_pic_sets: Vec<ShortTermRps> =
        Vec::with_capacity(num_short_term_ref_pic_sets as usize);
    for i in 0..num_short_term_ref_pic_sets as usize {
        let rps = parse_st_ref_pic_set(
            &mut r,
            i,
            num_short_term_ref_pic_sets as usize,
            &st_ref_pic_sets,
        )?;
        // 7.4.8: a set lists at most sps_max_dec_pic_buffering_minus1
        // pictures (each one a picture kept in the DPB).
        if rps.num_delta_pocs() > sps_max_dec_pic_buffering_minus1 as usize {
            return Err(DecodeError::InvalidSyntax(
                "st_ref_pic_set larger than the DPB",
            ));
        }
        st_ref_pic_sets.push(rps);
    }

    let long_term_ref_pics_present_flag = r.read_bit()? == 1;
    // Phase 3d-1: parse SPS long-term reference picture info but do nothing
    // with it yet (actual use is Phase 3d-2+).
    let mut num_long_term_ref_pics_sps: u32 = 0;
    let mut lt_ref_pic_poc_lsb_sps: Vec<u32> = Vec::new();
    let mut used_by_curr_pic_lt_sps_flag: Vec<bool> = Vec::new();
    if long_term_ref_pics_present_flag {
        num_long_term_ref_pics_sps = r.read_ue()?;
        if num_long_term_ref_pics_sps as usize > MAX_LONG_TERM_REF_PICS_SPS {
            return Err(DecodeError::InvalidSyntax(
                "num_long_term_ref_pics_sps out of range",
            ));
        }
        lt_ref_pic_poc_lsb_sps.reserve(num_long_term_ref_pics_sps as usize);
        used_by_curr_pic_lt_sps_flag.reserve(num_long_term_ref_pics_sps as usize);
        for _ in 0..num_long_term_ref_pics_sps {
            lt_ref_pic_poc_lsb_sps.push(r.read_bits(log2_max_pic_order_cnt_lsb)?);
            used_by_curr_pic_lt_sps_flag.push(r.read_bit()? == 1);
        }
    }

    let sps_temporal_mvp_enabled_flag = r.read_bit()? == 1;
    let strong_intra_smoothing_enabled_flag = r.read_bit()? == 1;

    // VUI, then the SPS extensions (spec 7.3.2.2.1). Only the range
    // extension's tool flags matter: coding tools this decoder does not
    // implement are rejected instead of being silently ignored.
    let range = match parse_vui_and_extensions(&mut r, sps_max_sub_layers_minus1) {
        Ok(range) => range,
        // A Main, Main 10 or Main Still Picture SPS cannot enable extension
        // tools, so a VUI this parser cannot follow is harmless there. For
        // other profiles the tool flags are unknown: refuse rather than
        // risk decoding with tools ignored.
        Err(_) if ptl.general_profile_idc <= 3 => SpsRangeExtension::default(),
        Err(e @ DecodeError::Unsupported(_)) => return Err(e),
        Err(_) => {
            return Err(DecodeError::Unsupported(
                "SPS VUI / extension data could not be parsed",
            ));
        }
    };

    Ok(Sps {
        sps_video_parameter_set_id,
        sps_max_sub_layers_minus1,
        sps_temporal_id_nesting_flag,
        profile_tier_level: ptl,
        sps_seq_parameter_set_id,
        chroma_format_idc,
        separate_colour_plane_flag,
        pic_width_in_luma_samples,
        pic_height_in_luma_samples,
        conf_win_left_offset,
        conf_win_right_offset,
        conf_win_top_offset,
        conf_win_bottom_offset,
        bit_depth_luma,
        bit_depth_chroma,
        log2_max_pic_order_cnt_lsb,
        min_cb_log2_size_y,
        ctb_log2_size_y,
        ctb_size_y,
        min_tb_log2_size_y,
        max_tb_log2_size_y,
        max_transform_hierarchy_depth_inter,
        max_transform_hierarchy_depth_intra,
        scaling_list_enabled_flag,
        scaling_list,
        amp_enabled_flag,
        sample_adaptive_offset_enabled_flag,
        pcm_enabled_flag,
        pcm_sample_bit_depth_luma,
        pcm_sample_bit_depth_chroma,
        log2_min_pcm_cb_size,
        log2_max_pcm_cb_size,
        pcm_loop_filter_disabled_flag,
        num_short_term_ref_pic_sets,
        st_ref_pic_sets,
        long_term_ref_pics_present_flag,
        num_long_term_ref_pics_sps,
        lt_ref_pic_poc_lsb_sps,
        used_by_curr_pic_lt_sps_flag,
        sps_temporal_mvp_enabled_flag,
        strong_intra_smoothing_enabled_flag,
        sps_max_dec_pic_buffering_minus1,
        sps_max_num_reorder_pics,
        sps_max_latency_increase_plus1,
        range,
    })
}
