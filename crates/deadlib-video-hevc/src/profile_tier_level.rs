//! HEVC `profile_tier_level()` syntax structure (spec 7.3.3, semantics 7.4.4).
//!
//! Used by both VPS and SPS to advertise the bitstream's profile, tier and level.
//! For Phase 1 we only extract the headline fields needed by downstream code
//! (`general_profile_idc`, `general_tier_flag`, `general_level_idc`); the rest
//! of the structure is consumed but discarded.

use crate::bitstream::BitstreamReader;
use crate::error::DecodeError;

#[derive(Debug, Clone, Copy)]
pub struct ProfileTierLevel {
    pub general_profile_space: u8,
    pub general_tier_flag: bool,
    pub general_profile_idc: u8,
    pub general_level_idc: u8,
}

/// Parse `profile_tier_level(profilePresentFlag=1, maxNumSubLayersMinus1)`.
///
/// Spec 7.3.3. The structure is fixed-size given `max_sub_layers_minus1`:
/// - 96 bits for the "general" portion (profile_space..level_idc)
/// - 16 bits of presence flags + 2-bit reserved padding when sub layers exist
/// - per-sub-layer profile/level structures (variable, gated by presence flags)
///
/// We require `profilePresentFlag = 1` here, which is the case for both
/// VPS and SPS callers in HEVC.
pub fn parse_profile_tier_level(
    reader: &mut BitstreamReader,
    max_sub_layers_minus1: u8,
) -> Result<ProfileTierLevel, DecodeError> {
    // ---- General profile/tier/level (96 bits) ----
    let general_profile_space = reader.read_bits(2)? as u8;
    let general_tier_flag = reader.read_bit()? == 1;
    let mut general_profile_idc = reader.read_bits(5)? as u8;

    // 32 profile compatibility flags. With general_profile_idc 0 the profile
    // is the first one flagged compatible (other than 0), as FFmpeg takes it.
    let compatibility = reader.read_bits(32)?;
    if general_profile_idc == 0 {
        // Flag j is bit 31 - j of the 32 bits read.
        if let Some(j) = (1..32).find(|&j| compatibility & (1 << (31 - j)) != 0) {
            general_profile_idc = j as u8;
        }
    }

    // 4 source/constraint flags.
    let _general_progressive_source_flag = reader.read_bit()?;
    let _general_interlaced_source_flag = reader.read_bit()?;
    let _general_non_packed_constraint_flag = reader.read_bit()?;
    let _general_frame_only_constraint_flag = reader.read_bit()?;

    // 43 bits of constraint flags (their interpretation depends on profile_idc /
    // compatibility flags) plus 1 inbld_flag/reserved bit. We don't need any of
    // them in Phase 1, so just consume 44 bits.
    //
    // Done as 22 + 22 since `read_bits` takes a u8 count up to 32. (43 fits, but
    // splitting matches the natural inbld split — 43 + 1 = 44.)
    let _ = reader.read_bits(22)?;
    let _ = reader.read_bits(22)?;

    let general_level_idc = reader.read_bits(8)? as u8;

    // ---- Sub-layer presence flags (only when there are sub layers) ----
    // For each sub layer below max, two presence flags. Then 2-bit reserved
    // padding to byte-align the sub-layer block to 8 entries.
    if max_sub_layers_minus1 > 0 {
        // sub_layer_profile_present_flag[i], sub_layer_level_present_flag[i]
        let mut sub_layer_profile_present = [false; 7];
        let mut sub_layer_level_present = [false; 7];
        for i in 0..max_sub_layers_minus1 as usize {
            sub_layer_profile_present[i] = reader.read_bit()? == 1;
            sub_layer_level_present[i] = reader.read_bit()? == 1;
        }
        // Reserved 2 bits per "missing" sub layer to make this region always
        // 16 bits regardless of max_sub_layers_minus1.
        for _ in (max_sub_layers_minus1 as usize)..8 {
            let _ = reader.read_bits(2)?;
        }

        // Per-sub-layer profile/level structures. Each present sub-layer profile
        // is identical in shape to the general portion above (88 bits without
        // the 8-bit level), and each present level is 8 bits.
        for i in 0..max_sub_layers_minus1 as usize {
            if sub_layer_profile_present[i] {
                // 2 + 1 + 5 + 32 + 4 + 43 + 1 = 88 bits
                let _ = reader.read_bits(2)?; // sub_layer_profile_space
                let _ = reader.read_bit()?; // sub_layer_tier_flag
                let _ = reader.read_bits(5)?; // sub_layer_profile_idc
                let _ = reader.read_bits(32)?; // sub_layer_profile_compatibility_flag[32]
                let _ = reader.read_bits(4)?; // four source/constraint flags
                let _ = reader.read_bits(22)?; // 22 + 22 of constraint/reserved
                let _ = reader.read_bits(22)?;
            }
            if sub_layer_level_present[i] {
                let _ = reader.read_bits(8)?; // sub_layer_level_idc
            }
        }
    }

    Ok(ProfileTierLevel {
        general_profile_space,
        general_tier_flag,
        general_profile_idc,
        general_level_idc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// general_profile_idc 0 takes the first profile flagged compatible
    /// (FFmpeg `decode_profile_tier_level`); a non-zero one is kept.
    #[test]
    fn profile_zero_inferred_from_compatibility_flags() {
        for (profile, flags, expected) in [(0u8, (1u32 << 29) | (1 << 27), 2u8), (1, 1 << 27, 1)] {
            // profile_space 0, tier 0, profile_idc; 32 flags (flag j is bit
            // 31 - j); 48 constraint bits; level 0: 96 bits in all.
            let mut bytes = vec![profile];
            bytes.extend_from_slice(&flags.to_be_bytes());
            bytes.extend_from_slice(&[0; 7]);
            let mut r = BitstreamReader::new(&bytes);
            let ptl = parse_profile_tier_level(&mut r, 0).expect("PTL");
            assert_eq!(ptl.general_profile_idc, expected);
        }
    }
}
