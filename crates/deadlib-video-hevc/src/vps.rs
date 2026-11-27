//! HEVC Video Parameter Set parsing (spec 7.3.2.1, semantics 7.4.3.1).
//!
//! For Phase 1 we parse just enough of the VPS to walk past it cleanly and
//! pull out the headline `vps_video_parameter_set_id`. The DPB-relevant
//! `vps_max_*` arrays are read but otherwise unused at this stage.

use crate::bitstream::BitstreamReader;
use crate::error::DecodeError;
use crate::profile_tier_level::{ProfileTierLevel, parse_profile_tier_level};

#[derive(Debug, Clone)]
pub struct Vps {
    pub vps_video_parameter_set_id: u8,
    pub vps_max_layers_minus1: u8,
    pub vps_max_sub_layers_minus1: u8,
    pub vps_temporal_id_nesting_flag: bool,
    pub profile_tier_level: ProfileTierLevel,
    /// `vps_max_dec_pic_buffering_minus1[i]`, indexed by sub-layer.
    /// Only entries `[0..=vps_max_sub_layers_minus1]` are meaningful.
    pub max_dec_pic_buffering_minus1: [u32; 7],
    pub max_num_reorder_pics: [u32; 7],
    pub max_latency_increase_plus1: [u32; 7],
}

pub fn parse_vps(rbsp: &[u8]) -> Result<Vps, DecodeError> {
    let mut r = BitstreamReader::new(rbsp);

    let vps_video_parameter_set_id = r.read_bits(4)? as u8;
    let _vps_base_layer_internal_flag = r.read_bit()?;
    let _vps_base_layer_available_flag = r.read_bit()?;
    let vps_max_layers_minus1 = r.read_bits(6)? as u8;
    let vps_max_sub_layers_minus1 = r.read_bits(3)? as u8;
    if vps_max_sub_layers_minus1 > 6 {
        return Err(DecodeError::InvalidSyntax(
            "vps_max_sub_layers_minus1 out of range",
        ));
    }
    let vps_temporal_id_nesting_flag = r.read_bit()? == 1;
    let vps_reserved_0xffff_16bits = r.read_bits(16)?;
    if vps_reserved_0xffff_16bits != 0xFFFF {
        return Err(DecodeError::InvalidSyntax(
            "vps_reserved_0xffff_16bits != 0xFFFF",
        ));
    }

    let ptl = parse_profile_tier_level(&mut r, vps_max_sub_layers_minus1)?;

    let vps_sub_layer_ordering_info_present_flag = r.read_bit()? == 1;
    let mut max_dec_pic_buffering_minus1 = [0u32; 7];
    let mut max_num_reorder_pics = [0u32; 7];
    let mut max_latency_increase_plus1 = [0u32; 7];
    let i_start = if vps_sub_layer_ordering_info_present_flag {
        0
    } else {
        vps_max_sub_layers_minus1 as usize
    };
    for i in i_start..=vps_max_sub_layers_minus1 as usize {
        max_dec_pic_buffering_minus1[i] = r.read_ue()?;
        max_num_reorder_pics[i] = r.read_ue()?;
        max_latency_increase_plus1[i] = r.read_ue()?;
    }

    // We don't need anything past this point in Phase 1, but we still walk
    // through it so the parser fully consumes the VPS in case future code
    // wants to verify trailing-bit alignment.
    let vps_max_layer_id = r.read_bits(6)? as u8;
    let vps_num_layer_sets_minus1 = r.read_ue()?;
    for _ in 1..=vps_num_layer_sets_minus1 {
        for _ in 0..=vps_max_layer_id {
            let _ = r.read_bit()?;
        }
    }

    let vps_timing_info_present_flag = r.read_bit()? == 1;
    if vps_timing_info_present_flag {
        // We don't care about timing info — just consume it.
        let _vps_num_units_in_tick = r.read_bits(32)?;
        let _vps_time_scale = r.read_bits(32)?;
        let vps_poc_proportional_to_timing_flag = r.read_bit()? == 1;
        if vps_poc_proportional_to_timing_flag {
            let _vps_num_ticks_poc_diff_one_minus1 = r.read_ue()?;
        }
        // hrd_parameters() (and the extension flag after them) are not
        // needed: stop here rather than fail a valid VPS.
        let _vps_num_hrd_parameters = r.read_ue()?;
    }

    Ok(Vps {
        vps_video_parameter_set_id,
        vps_max_layers_minus1,
        vps_max_sub_layers_minus1,
        vps_temporal_id_nesting_flag,
        profile_tier_level: ptl,
        max_dec_pic_buffering_minus1,
        max_num_reorder_pics,
        max_latency_increase_plus1,
    })
}
