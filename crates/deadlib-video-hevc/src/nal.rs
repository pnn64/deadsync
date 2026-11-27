//! HEVC NAL unit parsing — Annex B and HVCC (length-prefixed) formats.
//!
//! HEVC NAL header is 2 bytes (vs. 1 byte in H.264):
//!
//! ```text
//!   bit  0       1               7 8           13 14                  15
//!       +-+-------------------------+-------------+----------------------+
//!       |F|     nal_unit_type       | nuh_layer_id| nuh_temporal_id_plus1|
//!       +-+-------------------------+-------------+----------------------+
//!         (6 bits)                   (6 bits)      (3 bits)
//! ```
//!
//! - `forbidden_zero_bit` (F): must be 0.
//! - `nal_unit_type`: 6 bits, values 0..63 (HEVC spec table 7-1).
//! - `nuh_layer_id`: 6 bits. 0 for base layer; non-zero only for SHVC/MV-HEVC.
//! - `nuh_temporal_id_plus1`: 3 bits, must be > 0. `temporal_id = value - 1`.

use std::borrow::Cow;

/// HEVC NAL unit type (HEVC spec table 7-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NalUnitType {
    // VCL (slice) NAL units.
    /// TRAIL_N: trailing picture, sub-layer non-reference. Type 0.
    TrailN,
    /// TRAIL_R: trailing picture, sub-layer reference. Type 1.
    TrailR,
    /// TSA_N: temporal sub-layer access picture, sub-layer non-reference. Type 2.
    TsaN,
    /// TSA_R: temporal sub-layer access picture, sub-layer reference. Type 3.
    TsaR,
    /// STSA_N: step-wise temporal sub-layer access picture, sub-layer non-reference. Type 4.
    StsaN,
    /// STSA_R: step-wise temporal sub-layer access picture, sub-layer reference. Type 5.
    StsaR,
    /// RADL_N: random access decodable leading picture, sub-layer non-reference. Type 6.
    RadlN,
    /// RADL_R: random access decodable leading picture, sub-layer reference. Type 7.
    RadlR,
    /// RASL_N: random access skipped leading picture, sub-layer non-reference. Type 8.
    RaslN,
    /// RASL_R: random access skipped leading picture, sub-layer reference. Type 9.
    RaslR,
    /// BLA_W_LP: broken link access picture, may have leading pictures. Type 16.
    BlaWLp,
    /// BLA_W_RADL: broken link access picture, may have RADL leading pictures. Type 17.
    BlaWRadl,
    /// BLA_N_LP: broken link access picture without leading pictures. Type 18.
    BlaNLp,
    /// IDR_W_RADL: instantaneous decoding refresh picture, may have RADL leading pictures. Type 19.
    IdrWRadl,
    /// IDR_N_LP: instantaneous decoding refresh picture without leading pictures. Type 20.
    IdrNLp,
    /// CRA_NUT: clean random access picture. Type 21.
    Cra,
    // Non-VCL NAL units.
    /// VPS_NUT: video parameter set. Type 32.
    Vps,
    /// SPS_NUT: sequence parameter set. Type 33.
    Sps,
    /// PPS_NUT: picture parameter set. Type 34.
    Pps,
    /// AUD_NUT: access unit delimiter. Type 35.
    Aud,
    /// EOS_NUT: end of sequence. Type 36.
    Eos,
    /// EOB_NUT: end of bitstream. Type 37.
    Eob,
    /// FD_NUT: filler data. Type 38.
    Fd,
    /// PREFIX_SEI_NUT: supplemental enhancement information, prefix. Type 39.
    PrefixSei,
    /// SUFFIX_SEI_NUT: supplemental enhancement information, suffix. Type 40.
    SuffixSei,
    /// Any other type: reserved or unspecified values, which the decoder
    /// ignores. Holds the raw `nal_unit_type`.
    Other(u8),
}

impl NalUnitType {
    /// True for slice NAL types (VCL: 0..31). The values 22..23 (RSV_IRAP_VCL)
    /// and 24..31 (RSV_VCL) are also slice types but reserved.
    pub fn is_vcl(self) -> bool {
        match self {
            NalUnitType::TrailN
            | NalUnitType::TrailR
            | NalUnitType::TsaN
            | NalUnitType::TsaR
            | NalUnitType::StsaN
            | NalUnitType::StsaR
            | NalUnitType::RadlN
            | NalUnitType::RadlR
            | NalUnitType::RaslN
            | NalUnitType::RaslR
            | NalUnitType::BlaWLp
            | NalUnitType::BlaWRadl
            | NalUnitType::BlaNLp
            | NalUnitType::IdrWRadl
            | NalUnitType::IdrNLp
            | NalUnitType::Cra => true,
            // Reserved VCL types (RSV_VCL_N10..RSV_VCL31) are not decodable;
            // decoders shall ignore them (spec 7.4.2.2).
            NalUnitType::Other(_) => false,
            _ => false,
        }
    }

    /// True for IDR pictures (types 19, 20).
    pub fn is_idr(self) -> bool {
        matches!(self, NalUnitType::IdrWRadl | NalUnitType::IdrNLp)
    }

    /// True for CRA (Clean Random Access) pictures.
    pub fn is_cra(self) -> bool {
        matches!(self, NalUnitType::Cra)
    }

    /// True for IRAP (Intra Random Access Point) pictures: BLA, IDR, CRA.
    /// These are the only NAL types where decoding can start cleanly.
    pub fn is_irap(self) -> bool {
        matches!(
            self,
            NalUnitType::BlaWLp
                | NalUnitType::BlaWRadl
                | NalUnitType::BlaNLp
                | NalUnitType::IdrWRadl
                | NalUnitType::IdrNLp
                | NalUnitType::Cra
        )
    }
}

impl From<u8> for NalUnitType {
    fn from(val: u8) -> Self {
        match val {
            0 => NalUnitType::TrailN,
            1 => NalUnitType::TrailR,
            2 => NalUnitType::TsaN,
            3 => NalUnitType::TsaR,
            4 => NalUnitType::StsaN,
            5 => NalUnitType::StsaR,
            6 => NalUnitType::RadlN,
            7 => NalUnitType::RadlR,
            8 => NalUnitType::RaslN,
            9 => NalUnitType::RaslR,
            16 => NalUnitType::BlaWLp,
            17 => NalUnitType::BlaWRadl,
            18 => NalUnitType::BlaNLp,
            19 => NalUnitType::IdrWRadl,
            20 => NalUnitType::IdrNLp,
            21 => NalUnitType::Cra,
            32 => NalUnitType::Vps,
            33 => NalUnitType::Sps,
            34 => NalUnitType::Pps,
            35 => NalUnitType::Aud,
            36 => NalUnitType::Eos,
            37 => NalUnitType::Eob,
            38 => NalUnitType::Fd,
            39 => NalUnitType::PrefixSei,
            40 => NalUnitType::SuffixSei,
            v => NalUnitType::Other(v),
        }
    }
}

/// One NAL unit, as produced by [`parse_annex_b`], [`parse_hvcc`] or
/// [`parse_nal`] and consumed by
/// [`Decoder::decode_nal`](crate::Decoder::decode_nal).
#[derive(Debug)]
pub struct NalUnit<'a> {
    /// `nal_unit_type` from the NAL unit header.
    pub nal_unit_type: NalUnitType,
    /// `nuh_layer_id` from the NAL unit header: 0 for the base layer, the
    /// only layer this decoder decodes.
    pub nuh_layer_id: u8,
    /// `temporal_id` derived as `nuh_temporal_id_plus1 - 1`. Always 0..6.
    pub temporal_id: u8,
    /// RBSP payload (the bytes after the 2-byte NAL header, with emulation
    /// prevention bytes removed). Borrowed from the input when no EPB are
    /// present, owned otherwise.
    pub rbsp: Cow<'a, [u8]>,
    /// NAL-space byte positions (relative to the NAL payload start — i.e.
    /// after the 2-byte NAL header) at which an emulation prevention byte
    /// (0x03) was removed. Empty when no EPBs were present. HEVC spec
    /// 7.4.7.1 `entry_point_offset_minus1[]` is defined in NAL-space units
    /// (it counts EPBs); consumers that index into `rbsp` using those
    /// offsets must subtract the count of EPBs whose position is ≤ the
    /// target NAL offset to convert NAL → RBSP addresses.
    pub epb_positions: Vec<u32>,
}

/// Split an Annex B bytestream into NAL units.
/// Handles both 3-byte (`00 00 01`) and 4-byte (`00 00 00 01`) start codes.
pub fn parse_annex_b(data: &[u8]) -> Vec<NalUnit<'_>> {
    let Some((start, _)) = find_start_code(data, 0) else {
        return Vec::new();
    };
    annex_b_units(data, start).filter_map(parse_nal).collect()
}

/// Yields raw NAL units beginning immediately after the first start code.
pub(crate) fn annex_b_units(data: &[u8], start: usize) -> impl Iterator<Item = &[u8]> {
    let mut start = Some(start);
    core::iter::from_fn(move || {
        let first = start?;
        let next = find_start_code(data, first);
        start = next.map(|(after_prefix, _)| after_prefix);
        let mut payload = &data[first..next.map_or(data.len(), |(_, prefix)| prefix)];
        if next.is_some() {
            while let Some((&0, rest)) = payload.split_last() {
                payload = rest;
            }
        }
        Some(payload)
    })
}

/// Find the next start code starting from `offset`.
/// Returns (position after start code, position of start code beginning).
pub(crate) fn find_start_code(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    let mut i = offset;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                return Some((i + 3, i));
            }
            if i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                return Some((i + 4, i));
            }
        }
        i += 1;
    }
    None
}

/// Remove emulation prevention bytes (0x03 in `00 00 03` sequences).
/// Returns a borrowed slice when no emulation prevention bytes are found
/// (the common case), avoiding allocation entirely. Also returns the
/// NAL-space byte positions where each EPB was located (empty on the
/// fast path).
fn remove_emulation_prevention(data: &[u8]) -> (Cow<'_, [u8]>, Vec<u32>) {
    // Fast path: scan for 00 00 03. If none found, return borrowed slice.
    let has_epb = data.windows(3).any(|w| w[0] == 0 && w[1] == 0 && w[2] == 3);
    if !has_epb {
        return (Cow::Borrowed(data), Vec::new());
    }

    // Slow path: copy with emulation prevention removal.
    let mut rbsp = Vec::with_capacity(data.len());
    let mut epb_positions = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
            rbsp.push(0);
            rbsp.push(0);
            // Record the NAL-space position of the removed 0x03 byte.
            epb_positions.push((i + 2) as u32);
            i += 3; // skip the 0x03 byte
        } else {
            rbsp.push(data[i]);
            i += 1;
        }
    }
    (Cow::Owned(rbsp), epb_positions)
}

/// Parse a single raw NAL unit (no start code, no length prefix).
///
/// `nal_data` should start with the 2-byte NAL header. Returns `None` if
/// the data is too short or the header is malformed (forbidden bit set,
/// `nuh_temporal_id_plus1 == 0`).
pub fn parse_nal(nal_data: &[u8]) -> Option<NalUnit<'_>> {
    if nal_data.len() < 2 {
        return None;
    }
    let b0 = nal_data[0];
    let b1 = nal_data[1];
    if b0 & 0x80 != 0 {
        return None; // forbidden_zero_bit set
    }
    let nal_unit_type = NalUnitType::from((b0 >> 1) & 0x3F);
    let nuh_layer_id = ((b0 & 0x01) << 5) | (b1 >> 3);
    let temporal_id_plus1 = b1 & 0x07;
    if temporal_id_plus1 == 0 {
        return None;
    }
    let temporal_id = temporal_id_plus1 - 1;
    let (rbsp, epb_positions) = remove_emulation_prevention(&nal_data[2..]);
    Some(NalUnit {
        nal_unit_type,
        nuh_layer_id,
        temporal_id,
        rbsp,
        epb_positions,
    })
}

/// Split length-prefixed NAL units (HVCC / hvcC format from MP4/MKV containers).
///
/// Each NAL unit in `data` is preceded by a big-endian length field of
/// `length_size` bytes (typically 4, from `HEVCDecoderConfigurationRecord.
/// lengthSizeMinusOne + 1`). This is the format produced by MP4/MKV demuxers
/// for each video packet.
///
/// Unlike Annex B, HVCC data does NOT contain start codes and does NOT
/// require emulation-prevention-byte handling within the length field itself
/// (EPBs still exist within the NAL payload and are removed as usual).
///
/// Returns an empty vec if `length_size` is 0 or > 4.
///
/// # Example
///
/// ```
/// use deadlib_video_hevc::parse_hvcc;
///
/// // Minimal VPS NAL: 4-byte length (6) + 2-byte header + 4 bytes payload
/// let data = [
///     0x00, 0x00, 0x00, 0x06,  // length = 6
///     0x40, 0x01,              // NAL header: VPS, layer 0, tid 0
///     0x00, 0x00, 0x00, 0x00,  // payload
/// ];
/// let nals = parse_hvcc(&data, 4);
/// assert_eq!(nals.len(), 1);
/// assert_eq!(nals[0].nal_unit_type, deadlib_video_hevc::NalUnitType::Vps);
/// ```
pub fn parse_hvcc(data: &[u8], length_size: u8) -> Vec<NalUnit<'_>> {
    if length_size == 0 || length_size > 4 {
        return Vec::new();
    }
    let ls = length_size as usize;
    let mut nals = Vec::new();
    let mut i = 0;

    while i + ls <= data.len() {
        // Read big-endian length.
        let mut nal_len: usize = 0;
        for j in 0..ls {
            nal_len = (nal_len << 8) | data[i + j] as usize;
        }
        i += ls;

        if nal_len == 0 || nal_len > data.len() - i {
            break;
        }

        let nal_data = &data[i..i + nal_len];
        if let Some(nal) = parse_nal(nal_data) {
            nals.push(nal);
        }
        i += nal_len;
    }

    nals
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 2-byte HEVC NAL header for the given type / layer / temporal_id.
    fn hdr(nut: u8, layer_id: u8, temporal_id: u8) -> [u8; 2] {
        let b0 = (nut & 0x3F) << 1 | ((layer_id >> 5) & 0x01);
        let b1 = ((layer_id & 0x1F) << 3) | ((temporal_id + 1) & 0x07);
        [b0, b1]
    }

    #[test]
    fn test_header_packing_round_trip() {
        // VPS, layer 0, temporal_id 0
        let h = hdr(32, 0, 0);
        assert_eq!(h, [0x40, 0x01]);

        // SPS, layer 0, temporal_id 0
        let h = hdr(33, 0, 0);
        assert_eq!(h, [0x42, 0x01]);

        // IDR_W_RADL = 19, layer 0, temporal_id 0
        let h = hdr(19, 0, 0);
        assert_eq!(h, [0x26, 0x01]);
    }

    #[test]
    fn test_parse_annex_b_mixed_start_codes() {
        // Build a stream with one VPS (4-byte start code) and one SPS
        // (3-byte start code), each with a small payload.
        let mut data: Vec<u8> = Vec::new();
        // VPS NAL: 4-byte start code, header, payload
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        data.extend_from_slice(&hdr(32, 0, 0));
        data.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        // SPS NAL: 3-byte start code, header, payload
        data.extend_from_slice(&[0x00, 0x00, 0x01]);
        data.extend_from_slice(&hdr(33, 0, 0));
        data.extend_from_slice(&[0xDD, 0xEE]);

        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 2);

        assert_eq!(nals[0].nal_unit_type, NalUnitType::Vps);
        assert_eq!(nals[0].nuh_layer_id, 0);
        assert_eq!(nals[0].temporal_id, 0);
        assert_eq!(&*nals[0].rbsp, &[0xAA, 0xBB, 0xCC]);

        assert_eq!(nals[1].nal_unit_type, NalUnitType::Sps);
        assert_eq!(nals[1].nuh_layer_id, 0);
        assert_eq!(nals[1].temporal_id, 0);
        assert_eq!(&*nals[1].rbsp, &[0xDD, 0xEE]);
    }

    #[test]
    fn test_parse_annex_b_idr_with_temporal_id() {
        // IDR_W_RADL with temporal_id=2 should round-trip correctly.
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        data.extend_from_slice(&hdr(19, 0, 2));
        data.extend_from_slice(&[0x12, 0x34]);

        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_unit_type, NalUnitType::IdrWRadl);
        assert!(nals[0].nal_unit_type.is_idr());
        assert!(nals[0].nal_unit_type.is_irap());
        assert!(nals[0].nal_unit_type.is_vcl());
        assert_eq!(nals[0].temporal_id, 2);
    }

    #[test]
    fn test_parse_annex_b_strips_emulation_prevention() {
        // Build a NAL whose RBSP contains the EPB sequence 00 00 03 -> 00 00.
        // Use a non-zero header so the 00 00 in the payload doesn't accidentally
        // match a start code.
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        data.extend_from_slice(&hdr(33, 0, 0)); // SPS, [0x42, 0x01]
        // Payload: 0x00 0x00 0x03 0x42 0xAA  →  RBSP: 0x00 0x00 0x42 0xAA
        data.extend_from_slice(&[0x00, 0x00, 0x03, 0x42, 0xAA]);

        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_unit_type, NalUnitType::Sps);
        assert_eq!(&*nals[0].rbsp, &[0x00, 0x00, 0x42, 0xAA]);
        assert!(
            matches!(nals[0].rbsp, Cow::Owned(_)),
            "EPB removal should produce an owned buffer"
        );
    }

    #[test]
    fn test_parse_annex_b_zero_copy_when_no_epb() {
        // No emulation prevention bytes → rbsp should borrow from input.
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        data.extend_from_slice(&hdr(33, 0, 0));
        data.extend_from_slice(&[0x11, 0x22, 0x33]);

        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 1);
        assert!(
            matches!(nals[0].rbsp, Cow::Borrowed(_)),
            "no EPB → should borrow"
        );
    }

    #[test]
    fn test_parse_annex_b_skips_invalid_forbidden_bit() {
        // forbidden_zero_bit = 1 → NAL must be skipped.
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        // First NAL: forbidden bit set, should be ignored
        data.extend_from_slice(&[0x80, 0x01, 0xAA]);
        data.extend_from_slice(&[0x00, 0x00, 0x01]);
        // Second NAL: valid VPS
        data.extend_from_slice(&hdr(32, 0, 0));
        data.extend_from_slice(&[0xBB]);

        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_unit_type, NalUnitType::Vps);
    }

    #[test]
    fn test_nuh_layer_id_packing() {
        // Layer ID 33 = 0b100001 → MSB into byte0 LSB, 5 LSBs into byte1 high.
        let h = hdr(33, 33, 0);
        // byte0 = (33 << 1) | 1 = 0x42 | 1 = 0x43
        // byte1 = (33 & 0x1F) << 3 | 1 = 1 << 3 | 1 = 0x09
        assert_eq!(h, [0x43, 0x09]);

        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        data.extend_from_slice(&h);
        data.extend_from_slice(&[0xAA]);

        let nals = parse_annex_b(&data);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nuh_layer_id, 33);
    }

    // ---- HVCC (length-prefixed) tests ----

    #[test]
    fn test_parse_hvcc_single_nal() {
        let h = hdr(32, 0, 0); // VPS
        let payload = [0xAA, 0xBB, 0xCC];
        let nal_len: u32 = 2 + payload.len() as u32; // header + payload

        let mut data = Vec::new();
        data.extend_from_slice(&nal_len.to_be_bytes()); // 4-byte length
        data.extend_from_slice(&h);
        data.extend_from_slice(&payload);

        let nals = parse_hvcc(&data, 4);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_unit_type, NalUnitType::Vps);
        assert_eq!(nals[0].rbsp.as_ref(), &payload);
    }

    #[test]
    fn test_parse_hvcc_multiple_nals() {
        let mut data = Vec::new();

        // VPS
        let h_vps = hdr(32, 0, 0);
        let payload_vps = [0x01, 0x02];
        let len_vps: u32 = 2 + payload_vps.len() as u32;
        data.extend_from_slice(&len_vps.to_be_bytes());
        data.extend_from_slice(&h_vps);
        data.extend_from_slice(&payload_vps);

        // SPS
        let h_sps = hdr(33, 0, 0);
        let payload_sps = [0x03, 0x04, 0x05];
        let len_sps: u32 = 2 + payload_sps.len() as u32;
        data.extend_from_slice(&len_sps.to_be_bytes());
        data.extend_from_slice(&h_sps);
        data.extend_from_slice(&payload_sps);

        let nals = parse_hvcc(&data, 4);
        assert_eq!(nals.len(), 2);
        assert_eq!(nals[0].nal_unit_type, NalUnitType::Vps);
        assert_eq!(nals[1].nal_unit_type, NalUnitType::Sps);
        assert_eq!(nals[1].rbsp.as_ref(), &payload_sps);
    }

    #[test]
    fn test_parse_hvcc_2byte_length() {
        let h = hdr(34, 0, 0); // PPS
        let payload = [0xFF];
        let nal_len: u16 = 2 + payload.len() as u16;

        let mut data = Vec::new();
        data.extend_from_slice(&nal_len.to_be_bytes()); // 2-byte length
        data.extend_from_slice(&h);
        data.extend_from_slice(&payload);

        let nals = parse_hvcc(&data, 2);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_unit_type, NalUnitType::Pps);
    }

    #[test]
    fn test_parse_hvcc_with_epb() {
        let h = hdr(20, 0, 0); // IDR_N_LP
        // Payload with EPB: 00 00 03 01 → RBSP: 00 00 01
        let payload = [0x00, 0x00, 0x03, 0x01];
        let nal_len: u32 = 2 + payload.len() as u32;

        let mut data = Vec::new();
        data.extend_from_slice(&nal_len.to_be_bytes());
        data.extend_from_slice(&h);
        data.extend_from_slice(&payload);

        let nals = parse_hvcc(&data, 4);
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].rbsp.as_ref(), &[0x00, 0x00, 0x01]);
        assert_eq!(nals[0].epb_positions.len(), 1);
    }

    #[test]
    fn test_parse_hvcc_empty_and_invalid() {
        assert!(parse_hvcc(&[], 4).is_empty());
        assert!(parse_hvcc(&[0x00, 0x00], 4).is_empty()); // truncated length
        assert!(parse_hvcc(&[0x00, 0x00, 0x00, 0x00], 4).is_empty()); // zero length
        assert!(parse_hvcc(&[0x01], 0).is_empty()); // invalid length_size
        assert!(parse_hvcc(&[0x01], 5).is_empty()); // invalid length_size
    }

    #[test]
    fn test_parse_nal_single() {
        let h = hdr(33, 0, 0); // SPS
        let mut nal_data = Vec::new();
        nal_data.extend_from_slice(&h);
        nal_data.extend_from_slice(&[0xAA, 0xBB]);

        let nal = parse_nal(&nal_data).expect("should parse");
        assert_eq!(nal.nal_unit_type, NalUnitType::Sps);
        assert_eq!(nal.rbsp.as_ref(), &[0xAA, 0xBB]);
    }

    #[test]
    fn test_parse_nal_too_short() {
        assert!(parse_nal(&[]).is_none());
        assert!(parse_nal(&[0x00]).is_none());
    }

    #[test]
    fn test_parse_nal_forbidden_bit() {
        // forbidden_zero_bit = 1
        assert!(parse_nal(&[0x80, 0x01]).is_none());
    }
}
