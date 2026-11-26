use alloc::vec::Vec;

use crate::bitstream::BitstreamReader;

/// H.264 Table 7-2: Default 4x4 scaling list for Intra (in scan order).
#[rustfmt::skip]
pub const DEFAULT_SCALING_4X4_INTRA: [u8; 16] = [
     6, 13, 13, 20,
    20, 20, 28, 28,
    28, 28, 32, 32,
    32, 37, 37, 42,
];

/// H.264 Table 7-2: Default 4x4 scaling list for Inter (in scan order).
#[rustfmt::skip]
pub const DEFAULT_SCALING_4X4_INTER: [u8; 16] = [
    10, 14, 14, 20,
    20, 20, 24, 24,
    24, 24, 27, 27,
    27, 30, 30, 34,
];

/// H.264 Table 7-4: Default 8x8 scaling list for Intra (in scan order).
#[rustfmt::skip]
pub const DEFAULT_SCALING_8X8_INTRA: [u8; 64] = [
     6, 10, 10, 13, 11, 13, 16, 16,
    16, 16, 18, 18, 18, 18, 18, 23,
    23, 23, 23, 23, 23, 25, 25, 25,
    25, 25, 25, 25, 27, 27, 27, 27,
    27, 27, 27, 27, 29, 29, 29, 29,
    29, 29, 29, 31, 31, 31, 31, 31,
    31, 33, 33, 33, 33, 33, 36, 36,
    36, 36, 38, 38, 38, 40, 40, 42,
];

/// H.264 Table 7-4: Default 8x8 scaling list for Inter (in scan order).
#[rustfmt::skip]
pub const DEFAULT_SCALING_8X8_INTER: [u8; 64] = [
     9, 13, 13, 15, 13, 15, 17, 17,
    17, 17, 19, 19, 19, 19, 19, 21,
    21, 21, 21, 21, 21, 22, 22, 22,
    22, 22, 22, 22, 24, 24, 24, 24,
    24, 24, 24, 24, 25, 25, 25, 25,
    25, 25, 25, 27, 27, 27, 27, 27,
    27, 28, 28, 28, 28, 28, 30, 30,
    30, 30, 32, 32, 32, 33, 33, 35,
];

/// Default scaling list `i` of the SPS/PPS list order (0-5: 4x4 Intra Y,
/// Cb, Cr, Inter Y, Cb, Cr; 6, 7: 8x8 Intra Y, Inter Y), as a 64-entry
/// array whose first 16 entries hold a 4x4 list.
pub(crate) fn default_scaling_list(i: usize) -> [u8; 64] {
    let mut list = [0u8; 64];
    match i {
        0..=2 => list[..16].copy_from_slice(&DEFAULT_SCALING_4X4_INTRA),
        3..=5 => list[..16].copy_from_slice(&DEFAULT_SCALING_4X4_INTER),
        6 => list = DEFAULT_SCALING_8X8_INTRA,
        _ => list = DEFAULT_SCALING_8X8_INTER,
    }
    list
}

/// Scaling lists 0-7 (4x4 then 8x8) of a parameter set (spec 7.3.2.1.1.1,
/// 7.4.2.1.1, Table 7-2). An absent list falls back to `fallback_first(i)`
/// for lists 0, 3, 6 and 7 (the default list in an SPS, or the SPS list in
/// a PPS when the SPS carries matrices) and to the previous list of the same
/// kind otherwise. A list coded as "use default" takes the default list.
pub(crate) fn parse_scaling_matrices(
    r: &mut BitstreamReader,
    count: usize,
    fallback_first: impl Fn(usize) -> [u8; 64],
    lists_4x4: &mut [[u8; 16]; 6],
    lists_8x8: &mut [[u8; 64]; 2],
) -> Result<(), &'static str> {
    for i in 0..count {
        let present = r.read_bit()? != 0;
        let list: [u8; 64] = if present {
            let size = if i < 6 { 16 } else { 64 };
            let (list, use_default) = parse_scaling_list::<64>(r, size)?;
            if use_default {
                default_scaling_list(i)
            } else {
                list
            }
        } else if matches!(i, 0 | 3 | 6 | 7) {
            fallback_first(i)
        } else if i < 6 {
            let mut prev = [0u8; 64];
            prev[..16].copy_from_slice(&lists_4x4[i - 1]);
            prev
        } else {
            // 8x8 chroma lists of 4:4:4 (i >= 8): not used
            lists_8x8[(i - 6) % 2]
        };
        if i < 6 {
            lists_4x4[i].copy_from_slice(&list[..16]);
        } else if i < 8 {
            lists_8x8[i - 6] = list;
        }
    }
    // 8x8 lists not coded at all (count 6) still follow the fall-back rule
    for i in count.max(6)..8 {
        lists_8x8[i - 6] = fallback_first(i);
    }
    Ok(())
}

/// Flat scaling list (no custom scaling) — all 16s.
pub const FLAT_SCALING_4X4: [u8; 16] = [16; 16];

/// Sequence Parameter Set (H.264 spec section 7.3.2.1).
#[derive(Debug)]
pub struct Sps {
    pub profile_idc: u8,
    pub constraint_set0_flag: bool,
    pub constraint_set1_flag: bool,
    pub constraint_set2_flag: bool,
    pub constraint_set3_flag: bool,
    pub constraint_set4_flag: bool,
    pub constraint_set5_flag: bool,
    pub level_idc: u8,
    pub seq_parameter_set_id: u32,

    // High profile and above fields
    pub chroma_format_idc: u32,
    pub separate_colour_plane_flag: bool,
    pub bit_depth_luma_minus8: u32,
    pub bit_depth_chroma_minus8: u32,
    pub qpprime_y_zero_transform_bypass_flag: bool,
    pub seq_scaling_matrix_present_flag: bool,
    /// 4x4 scaling matrices [0..5]: Intra Y, Intra Cb, Intra Cr, Inter Y, Inter Cb, Inter Cr.
    /// Default is all 16s (flat scaling). Stored in zigzag scan order, as coded.
    pub scaling_list_4x4: [[u8; 16]; 6],
    /// 8x8 scaling matrices [0..1]: Intra Y, Inter Y (for 4:2:0).
    /// Default is all 16s. Stored in zigzag scan order, as coded.
    pub scaling_list_8x8: [[u8; 64]; 2],

    pub log2_max_frame_num_minus4: u32,
    pub pic_order_cnt_type: u32,
    pub log2_max_pic_order_cnt_lsb_minus4: u32,
    pub delta_pic_order_always_zero_flag: bool,
    pub offset_for_non_ref_pic: i32,
    pub offset_for_top_to_bottom_field: i32,
    pub num_ref_frames_in_pic_order_cnt_cycle: u32,
    pub offset_for_ref_frame: Vec<i32>,

    pub max_num_ref_frames: u32,
    pub gaps_in_frame_num_value_allowed_flag: bool,
    pub pic_width_in_mbs_minus1: u32,
    pub pic_height_in_map_units_minus1: u32,
    pub frame_mbs_only_flag: bool,
    pub mb_adaptive_frame_field_flag: bool,
    pub direct_8x8_inference_flag: bool,

    pub frame_cropping_flag: bool,
    pub frame_crop_left_offset: u32,
    pub frame_crop_right_offset: u32,
    pub frame_crop_top_offset: u32,
    pub frame_crop_bottom_offset: u32,

    pub vui_parameters_present_flag: bool,
    /// VUI colour interpretation, with H.264's unspecified defaults retained.
    pub video_full_range_flag: bool,
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub chroma_sample_loc_type_top_field: u32,
    /// Maximum reorder depth from VUI bitstream restrictions, when signalled.
    pub max_num_reorder_frames: Option<u32>,

    /// VUI timing info: numerator of clock tick. Set when
    /// `vui_parameters_present_flag` and `timing_info_present_flag` are both
    /// true. The frame rate is `time_scale / (2 * num_units_in_tick)` for
    /// progressive content (spec E.2.1).
    pub num_units_in_tick: Option<u32>,
    /// VUI timing info: denominator of clock tick (typically a multiple of
    /// the frame rate, e.g. 60000 for 29.97 fps).
    pub time_scale: Option<u32>,
    /// VUI timing info: when set, frames are emitted at a fixed rate of
    /// `time_scale / (2 * num_units_in_tick)`.
    pub fixed_frame_rate_flag: bool,
}

impl Sps {
    /// Frame rate from VUI timing info, if present, as `(numerator, denominator)`.
    ///
    /// For progressive content (the common case), this is
    /// `time_scale / (2 * num_units_in_tick)`. For example, an x264 stream
    /// at 29.97 fps would return `Some((60000, 2002))`.
    ///
    /// Returns `None` if the SPS does not contain VUI timing info, or if
    /// the values are zero (which would be a division-by-zero).
    pub fn frame_rate(&self) -> Option<(u32, u32)> {
        let num_units = self.num_units_in_tick?;
        let time_scale = self.time_scale?;
        if num_units == 0 || time_scale == 0 {
            return None;
        }
        // Spec E.2.1: clock_tick = num_units_in_tick / time_scale
        // For progressive frame coding, each frame is 2 ticks → fps = time_scale / (2 * num_units_in_tick)
        Some((time_scale, 2u32.saturating_mul(num_units)))
    }

    /// Frame rate as a single floating-point value, computed from
    /// [`frame_rate`](Self::frame_rate). Returns `None` if VUI timing info
    /// is not present.
    pub fn frame_rate_f64(&self) -> Option<f64> {
        let (num, den) = self.frame_rate()?;
        Some(num as f64 / den as f64)
    }

    /// Width in pixels (accounting for cropping).
    pub fn width(&self) -> u32 {
        let crop_x = if self.frame_cropping_flag {
            self.frame_crop_left_offset
                .saturating_add(self.frame_crop_right_offset)
                .saturating_mul(self.crop_unit_x())
        } else {
            0
        };
        self.pic_width_in_mbs_minus1
            .saturating_add(1)
            .saturating_mul(16)
            .saturating_sub(crop_x)
    }

    /// Height in pixels (accounting for cropping).
    pub fn height(&self) -> u32 {
        let crop_y = if self.frame_cropping_flag {
            self.frame_crop_top_offset
                .saturating_add(self.frame_crop_bottom_offset)
                .saturating_mul(self.crop_unit_y())
        } else {
            0
        };
        self.pic_height_in_map_units_minus1
            .saturating_add(1)
            .saturating_mul(16)
            .saturating_mul(if self.frame_mbs_only_flag { 1 } else { 2 })
            .saturating_sub(crop_y)
    }

    /// Luma samples cropped off the left edge (spec 7.4.2.1.1).
    pub fn crop_left(&self) -> u32 {
        if self.frame_cropping_flag {
            self.frame_crop_left_offset
                .saturating_mul(self.crop_unit_x())
        } else {
            0
        }
    }

    /// Luma frame lines cropped off the top edge (spec 7.4.2.1.1).
    pub fn crop_top(&self) -> u32 {
        if self.frame_cropping_flag {
            self.frame_crop_top_offset
                .saturating_mul(self.crop_unit_y())
        } else {
            0
        }
    }

    fn crop_unit_x(&self) -> u32 {
        if self.chroma_format_idc == 0 {
            1
        } else {
            // SubWidthC: 2 for 4:2:0 and 4:2:2, 1 for 4:4:4
            if self.chroma_format_idc == 3 {
                1
            } else {
                2
            }
        }
    }

    fn crop_unit_y(&self) -> u32 {
        let sub_height_c = if self.chroma_format_idc == 1 { 2 } else { 1 };
        let frame_factor = if self.frame_mbs_only_flag { 1 } else { 2 };
        if self.chroma_format_idc == 0 {
            frame_factor
        } else {
            sub_height_c * frame_factor
        }
    }
}

fn is_high_profile(profile_idc: u8) -> bool {
    matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    )
}

/// Parse an SPS from RBSP data (NAL header byte already stripped).
pub fn parse_sps(rbsp: &[u8]) -> Result<Sps, &'static str> {
    let mut r = BitstreamReader::new(rbsp);

    let profile_idc = r.read_bits(8)? as u8;
    let constraint_set0_flag = r.read_bit()? != 0;
    let constraint_set1_flag = r.read_bit()? != 0;
    let constraint_set2_flag = r.read_bit()? != 0;
    let constraint_set3_flag = r.read_bit()? != 0;
    let constraint_set4_flag = r.read_bit()? != 0;
    let constraint_set5_flag = r.read_bit()? != 0;
    let _reserved_zero_2bits = r.read_bits(2)?;
    let level_idc = r.read_bits(8)? as u8;
    let seq_parameter_set_id = r.read_ue()?;
    // Ranges of spec 7.4.2.1.1, checked here so later arithmetic on these
    // fields (shift widths, table sizes, list lengths) cannot overflow
    if seq_parameter_set_id > 31 {
        return Err("seq_parameter_set_id out of range");
    }

    let mut chroma_format_idc = 1; // default
    let mut separate_colour_plane_flag = false;
    let mut bit_depth_luma_minus8 = 0;
    let mut bit_depth_chroma_minus8 = 0;
    let mut qpprime_y_zero_transform_bypass_flag = false;
    let mut seq_scaling_matrix_present_flag = false;
    // Default: flat scaling (all 16s) when seq_scaling_matrix_present_flag is false
    let mut scaling_list_4x4 = [FLAT_SCALING_4X4; 6];
    let mut scaling_list_8x8 = [[16u8; 64]; 2];

    if is_high_profile(profile_idc) {
        chroma_format_idc = r.read_ue()?;
        if chroma_format_idc == 3 {
            separate_colour_plane_flag = r.read_bit()? != 0;
        }
        bit_depth_luma_minus8 = r.read_ue()?;
        bit_depth_chroma_minus8 = r.read_ue()?;
        if chroma_format_idc > 3 {
            return Err("chroma_format_idc out of range");
        }
        if bit_depth_luma_minus8 > 6 || bit_depth_chroma_minus8 > 6 {
            return Err("bit depth out of range");
        }
        qpprime_y_zero_transform_bypass_flag = r.read_bit()? != 0;
        seq_scaling_matrix_present_flag = r.read_bit()? != 0;
        if seq_scaling_matrix_present_flag {
            let count = if chroma_format_idc != 3 { 8 } else { 12 };
            // Fall-back rule A: absent lists take the defaults
            parse_scaling_matrices(
                &mut r,
                count,
                default_scaling_list,
                &mut scaling_list_4x4,
                &mut scaling_list_8x8,
            )?;
        }
    }

    let log2_max_frame_num_minus4 = r.read_ue()?;
    let pic_order_cnt_type = r.read_ue()?;
    if log2_max_frame_num_minus4 > 12 {
        return Err("log2_max_frame_num_minus4 out of range");
    }
    if pic_order_cnt_type > 2 {
        return Err("pic_order_cnt_type out of range");
    }

    let mut log2_max_pic_order_cnt_lsb_minus4 = 0;
    let mut delta_pic_order_always_zero_flag = false;
    let mut offset_for_non_ref_pic = 0;
    let mut offset_for_top_to_bottom_field = 0;
    let mut num_ref_frames_in_pic_order_cnt_cycle = 0;
    let mut offset_for_ref_frame = Vec::new();

    if pic_order_cnt_type == 0 {
        log2_max_pic_order_cnt_lsb_minus4 = r.read_ue()?;
        if log2_max_pic_order_cnt_lsb_minus4 > 12 {
            return Err("log2_max_pic_order_cnt_lsb_minus4 out of range");
        }
    } else if pic_order_cnt_type == 1 {
        delta_pic_order_always_zero_flag = r.read_bit()? != 0;
        offset_for_non_ref_pic = r.read_se()?;
        offset_for_top_to_bottom_field = r.read_se()?;
        num_ref_frames_in_pic_order_cnt_cycle = r.read_ue()?;
        if num_ref_frames_in_pic_order_cnt_cycle > 255 {
            return Err("num_ref_frames_in_pic_order_cnt_cycle out of range");
        }
        for _ in 0..num_ref_frames_in_pic_order_cnt_cycle {
            offset_for_ref_frame.push(r.read_se()?);
        }
    }

    let max_num_ref_frames = r.read_ue()?;
    // MaxDpbFrames is at most 16 (spec A.3.1)
    if max_num_ref_frames > 16 {
        return Err("max_num_ref_frames out of range");
    }
    let gaps_in_frame_num_value_allowed_flag = r.read_bit()? != 0;
    let pic_width_in_mbs_minus1 = r.read_ue()?;
    let pic_height_in_map_units_minus1 = r.read_ue()?;
    let frame_mbs_only_flag = r.read_bit()? != 0;

    let mut mb_adaptive_frame_field_flag = false;
    if !frame_mbs_only_flag {
        mb_adaptive_frame_field_flag = r.read_bit()? != 0;
    }

    let direct_8x8_inference_flag = r.read_bit()? != 0;

    let frame_cropping_flag = r.read_bit()? != 0;
    let mut frame_crop_left_offset = 0;
    let mut frame_crop_right_offset = 0;
    let mut frame_crop_top_offset = 0;
    let mut frame_crop_bottom_offset = 0;
    if frame_cropping_flag {
        frame_crop_left_offset = r.read_ue()?;
        frame_crop_right_offset = r.read_ue()?;
        frame_crop_top_offset = r.read_ue()?;
        frame_crop_bottom_offset = r.read_ue()?;
    }

    let vui_parameters_present_flag = r.read_bit()? != 0;
    let mut num_units_in_tick: Option<u32> = None;
    let mut time_scale: Option<u32> = None;
    let mut fixed_frame_rate_flag = false;
    let mut video_full_range_flag = false;
    let mut colour_primaries = 2;
    let mut transfer_characteristics = 2;
    let mut matrix_coefficients = 2;
    let mut chroma_sample_loc_type_top_field = 0;
    let mut max_num_reorder_frames = None;
    if vui_parameters_present_flag {
        // Parse just enough of the VUI to extract timing info (spec E.1.1).
        // We read the early optional sub-flags so we can reach
        // timing_info_present_flag, then parse the timing fields.
        let aspect_ratio_info_present_flag = r.read_bit()? != 0;
        if aspect_ratio_info_present_flag {
            let aspect_ratio_idc = r.read_bits(8)? as u8;
            if aspect_ratio_idc == 255 {
                // Extended_SAR: sar_width (16) + sar_height (16)
                r.skip_bits(16);
                r.skip_bits(16);
            }
        }
        let overscan_info_present_flag = r.read_bit()? != 0;
        if overscan_info_present_flag {
            r.skip_bits(1); // overscan_appropriate_flag
        }
        let video_signal_type_present_flag = r.read_bit()? != 0;
        if video_signal_type_present_flag {
            r.skip_bits(3); // video_format
            video_full_range_flag = r.read_bit()? != 0;
            let colour_description_present_flag = r.read_bit()? != 0;
            if colour_description_present_flag {
                colour_primaries = r.read_bits(8)? as u8;
                transfer_characteristics = r.read_bits(8)? as u8;
                matrix_coefficients = r.read_bits(8)? as u8;
            }
        }
        let chroma_loc_info_present_flag = r.read_bit()? != 0;
        if chroma_loc_info_present_flag {
            chroma_sample_loc_type_top_field = r.read_ue()?;
            let _ = r.read_ue()?; // chroma_sample_loc_type_bottom_field
        }
        let timing_info_present_flag = r.read_bit()? != 0;
        if timing_info_present_flag {
            // Both fields are 32 bits — read in two 16-bit halves since
            // read_bits takes u8 and we want to be safe with bit ordering.
            let hi = r.read_bits(16)?;
            let lo = r.read_bits(16)?;
            num_units_in_tick = Some((hi << 16) | lo);
            let hi = r.read_bits(16)?;
            let lo = r.read_bits(16)?;
            time_scale = Some((hi << 16) | lo);
            fixed_frame_rate_flag = r.read_bit()? != 0;
        }
        // The remaining VUI fields (HRD, bitstream_restriction) are not
        // needed for decoding, so a malformed or truncated tail must not
        // reject an otherwise valid SPS. Parse them best-effort.
        max_num_reorder_frames = parse_vui_tail(&mut r);
    }

    Ok(Sps {
        profile_idc,
        constraint_set0_flag,
        constraint_set1_flag,
        constraint_set2_flag,
        constraint_set3_flag,
        constraint_set4_flag,
        constraint_set5_flag,
        level_idc,
        seq_parameter_set_id,
        chroma_format_idc,
        separate_colour_plane_flag,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        qpprime_y_zero_transform_bypass_flag,
        seq_scaling_matrix_present_flag,
        scaling_list_4x4,
        scaling_list_8x8,
        log2_max_frame_num_minus4,
        pic_order_cnt_type,
        log2_max_pic_order_cnt_lsb_minus4,
        delta_pic_order_always_zero_flag,
        offset_for_non_ref_pic,
        offset_for_top_to_bottom_field,
        num_ref_frames_in_pic_order_cnt_cycle,
        offset_for_ref_frame,
        max_num_ref_frames,
        gaps_in_frame_num_value_allowed_flag,
        pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1,
        frame_mbs_only_flag,
        mb_adaptive_frame_field_flag,
        direct_8x8_inference_flag,
        frame_cropping_flag,
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
        vui_parameters_present_flag,
        video_full_range_flag,
        colour_primaries,
        transfer_characteristics,
        matrix_coefficients,
        chroma_sample_loc_type_top_field,
        max_num_reorder_frames,
        num_units_in_tick,
        time_scale,
        fixed_frame_rate_flag,
    })
}

/// Parse a scaling list from the bitstream (H.264 spec 7.3.2.1.1).
/// Returns `size` scale values in scan order, and whether the list asks for
/// the default list instead (useDefaultScalingMatrixFlag).
pub fn parse_scaling_list<const N: usize>(
    r: &mut BitstreamReader,
    size: usize,
) -> Result<([u8; N], bool), &'static str> {
    let mut scaling_list = [0u8; N];
    let mut last_scale: i32 = 8;
    let mut next_scale: i32 = 8;
    let mut use_default = false;
    for (j, entry) in scaling_list.iter_mut().take(size).enumerate() {
        if next_scale != 0 {
            let delta = r.read_se()?;
            next_scale = (last_scale.wrapping_add(delta).wrapping_add(256)).rem_euclid(256);
            use_default = j == 0 && next_scale == 0;
        }
        let val = if next_scale == 0 {
            last_scale
        } else {
            next_scale
        };
        *entry = val as u8;
        last_scale = val;
    }
    Ok((scaling_list, use_default))
}

/// Best-effort parse of the VUI fields after timing_info (spec E.1.1):
/// HRD parameters, pic_struct_present_flag and bitstream_restriction.
/// Returns `max_num_reorder_frames` when signalled; any parse failure
/// yields `None` rather than an error.
fn parse_vui_tail(r: &mut BitstreamReader) -> Option<u32> {
    let nal_hrd = r.read_bit().ok()? != 0;
    if nal_hrd {
        skip_hrd_parameters(r).ok()?;
    }
    let vcl_hrd = r.read_bit().ok()? != 0;
    if vcl_hrd {
        skip_hrd_parameters(r).ok()?;
    }
    if nal_hrd || vcl_hrd {
        r.skip_bits(1); // low_delay_hrd_flag
    }
    r.skip_bits(1); // pic_struct_present_flag
    if r.read_bit().ok()? == 0 {
        return None; // bitstream_restriction_flag
    }
    r.skip_bits(1); // motion_vectors_over_pic_boundaries_flag
    r.read_ue().ok()?; // max_bytes_per_pic_denom
    r.read_ue().ok()?; // max_bits_per_mb_denom
    r.read_ue().ok()?; // log2_max_mv_length_horizontal
    r.read_ue().ok()?; // log2_max_mv_length_vertical
    let max_num_reorder_frames = r.read_ue().ok()?;
    r.read_ue().ok()?; // max_dec_frame_buffering
    Some(max_num_reorder_frames)
}

// H.264 E.1.2; cpb_cnt_minus1 is bounded by the syntax specification.
fn skip_hrd_parameters(r: &mut BitstreamReader) -> Result<(), &'static str> {
    let cpb_count = r.read_ue()?;
    if cpb_count > 31 {
        return Err("invalid HRD CPB count");
    }
    r.skip_bits(8);
    for _ in 0..=cpb_count {
        r.read_ue()?;
        r.read_ue()?;
        r.skip_bits(1);
    }
    r.skip_bits(20);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Baseline 16x16 SPS (POC type 2), with each field overridable.
    pub(crate) fn baseline_sps(
        sps_id: u32,
        log2_max_frame_num_minus4: u32,
        poc_type: u32,
        max_num_ref_frames: u32,
    ) -> alloc::vec::Vec<u8> {
        let mut w = crate::bitstream::BitWriter::default();
        w.u(8, 66).u(8, 0).u(8, 10).ue(sps_id);
        w.ue(log2_max_frame_num_minus4).ue(poc_type);
        if poc_type == 0 {
            w.ue(0);
        }
        w.ue(max_num_ref_frames).u(1, 0).ue(0).ue(0);
        w.u(1, 1).u(1, 1).u(1, 0).u(1, 0);
        w.finish()
    }

    /// Out-of-range SPS fields are rejected when parsed (spec 7.4.2.1.1),
    /// including ue(v) values near u32::MAX that used to overflow `+ 4`.
    #[test]
    fn test_sps_field_ranges() {
        assert!(parse_sps(&baseline_sps(31, 12, 2, 16)).is_ok());
        for (rbsp, what) in [
            (baseline_sps(32, 0, 2, 1), "seq_parameter_set_id"),
            (baseline_sps(0, 13, 2, 1), "log2_max_frame_num_minus4"),
            (
                baseline_sps(0, u32::MAX - 1, 2, 1),
                "log2_max_frame_num_minus4",
            ),
            (baseline_sps(0, 0, 3, 1), "pic_order_cnt_type"),
            (baseline_sps(0, 0, 2, 17), "max_num_ref_frames"),
        ] {
            assert!(parse_sps(&rbsp).unwrap_err().starts_with(what), "{what}");
        }
    }

    #[test]
    fn test_frame_rate_returns_none_without_timing() {
        // Construct a synthetic Sps with no timing info to verify
        // frame_rate() returns None gracefully.
        let mut sps = Sps {
            profile_idc: 66,
            constraint_set0_flag: false,
            constraint_set1_flag: false,
            constraint_set2_flag: false,
            constraint_set3_flag: false,
            constraint_set4_flag: false,
            constraint_set5_flag: false,
            level_idc: 30,
            seq_parameter_set_id: 0,
            chroma_format_idc: 1,
            separate_colour_plane_flag: false,
            bit_depth_luma_minus8: 0,
            bit_depth_chroma_minus8: 0,
            qpprime_y_zero_transform_bypass_flag: false,
            seq_scaling_matrix_present_flag: false,
            scaling_list_4x4: [[16; 16]; 6],
            scaling_list_8x8: [[16; 64]; 2],
            log2_max_frame_num_minus4: 0,
            pic_order_cnt_type: 0,
            log2_max_pic_order_cnt_lsb_minus4: 0,
            delta_pic_order_always_zero_flag: false,
            offset_for_non_ref_pic: 0,
            offset_for_top_to_bottom_field: 0,
            num_ref_frames_in_pic_order_cnt_cycle: 0,
            offset_for_ref_frame: vec![],
            max_num_ref_frames: 1,
            gaps_in_frame_num_value_allowed_flag: false,
            pic_width_in_mbs_minus1: 0,
            pic_height_in_map_units_minus1: 0,
            frame_mbs_only_flag: true,
            mb_adaptive_frame_field_flag: false,
            direct_8x8_inference_flag: true,
            frame_cropping_flag: false,
            frame_crop_left_offset: 0,
            frame_crop_right_offset: 0,
            frame_crop_top_offset: 0,
            frame_crop_bottom_offset: 0,
            vui_parameters_present_flag: false,
            video_full_range_flag: false,
            colour_primaries: 2,
            transfer_characteristics: 2,
            matrix_coefficients: 2,
            chroma_sample_loc_type_top_field: 0,
            max_num_reorder_frames: None,
            num_units_in_tick: None,
            time_scale: None,
            fixed_frame_rate_flag: false,
        };
        assert!(sps.frame_rate().is_none());
        assert!(sps.frame_rate_f64().is_none());

        // Also: zero values should yield None (avoid division by zero)
        sps.num_units_in_tick = Some(0);
        sps.time_scale = Some(60);
        assert!(sps.frame_rate().is_none());
    }
}
