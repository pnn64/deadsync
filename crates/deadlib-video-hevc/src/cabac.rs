//! HEVC CABAC (Context-Adaptive Binary Arithmetic Coding) decoder.
//!
//! Implements the binary arithmetic decoder per HEVC spec section 9.3.4.
//! The arithmetic engine itself (rangeTabLPS, transIdxLPS/MPS, renormalization)
//! is mathematically identical to H.264 CABAC; HEVC inherited it verbatim.
//! Only the per-syntax-element context **init values** and the per-element
//! **context derivation rules** differ between H.264 and HEVC.
//!
//! State encoding follows the FFmpeg-style packed convention used by
//! `rust_h264`: a single `u8` per context where the low bit is `valMps`
//! (0 = MPS is 0, 1 = MPS is 1) and the upper bits encode `pStateIdx`.
//! Concretely the combined value is `2 * pStateIdx + valMps`, ranging
//! 0..=125 for legal states. Each decision bin does one lookup in
//! `CABAC_FUSED`, which packs rangeTabLps and both state transitions.

use crate::cabac_tables::{CABAC_FUSED, HEVC_CONTEXTS, INIT_VALUES};
use crate::slice::SliceType;

/// Bits buffered in the low register beyond the active range.
const CABAC_BITS: u32 = 16;
/// Mask for the buffered region of `low`.
const CABAC_MASK: u32 = (1 << CABAC_BITS) - 1;

/// Renormalisation shift: how far `range` (9 bits, non-zero) must move
/// left to reach 256. One count-leading-zeros instead of a table lookup on
/// the per-bin dependency chain.
#[inline]
fn norm_shift(range: u32) -> u32 {
    range.leading_zeros() - 23
}

/// HEVC CABAC binary arithmetic decoder.
///
/// Operates on the slice data RBSP **after** the slice header's byte
/// alignment. The caller is responsible for handing the right slice of bytes
/// (i.e. starting at the byte CABAC initialization should consume first).
pub struct CabacReader<'a> {
    low: u32,
    range: u32,
    data: &'a [u8],
    pos: usize,
}

impl<'a> CabacReader<'a> {
    /// Initialize the CABAC decoder from RBSP data at a given byte position
    /// (HEVC spec 9.3.2.2).
    ///
    /// The 2-byte aligned initialization matches the FFmpeg-style code: load
    /// two bytes into `low`, plant a fixed bias of `1 << 9` in place of the
    /// "third" byte, and let the first renormalization refill consume the
    /// next two bytes naturally.
    /// Returns `Err` if `byte_offset` is too close to the end of the data
    /// to read the 2 seed bytes (malformed bitstream).
    pub fn new(data: &'a [u8], byte_offset: usize) -> Result<Self, crate::error::DecodeError> {
        if byte_offset + 2 > data.len() {
            return Err(crate::error::DecodeError::InvalidSyntax(
                "CABAC init byte offset past end of RBSP",
            ));
        }
        let mut low: u32 = (data[byte_offset] as u32) << 18;
        low = low.wrapping_add((data[byte_offset + 1] as u32) << 10);
        low = low.wrapping_add(1 << 9);
        Ok(CabacReader {
            low,
            range: 0x1FE,
            data,
            pos: byte_offset + 2,
        })
    }

    /// Refill the buffered region of `low` after a renormalization that
    /// emptied it (spec 9.3.4.3.1 — the `read_bits()` step).
    #[inline]
    fn refill2(&mut self) {
        let i = self.low.trailing_zeros().wrapping_sub(CABAC_BITS);

        let b0 = if self.pos < self.data.len() {
            self.data[self.pos]
        } else {
            0
        };
        let b1 = if self.pos + 1 < self.data.len() {
            self.data[self.pos + 1]
        } else {
            0
        };
        let x = (b0 as u32) << 9 | (b1 as u32) << 1;
        let x = x.wrapping_sub(CABAC_MASK);
        self.low = self.low.wrapping_add(x << i);
        self.pos += 2;
    }

    /// Refill used by bypass / terminate paths (single renormalization step).
    #[inline]
    fn refill(&mut self) {
        let b0 = if self.pos < self.data.len() {
            self.data[self.pos]
        } else {
            0
        };
        let b1 = if self.pos + 1 < self.data.len() {
            self.data[self.pos + 1]
        } else {
            0
        };
        self.low = self.low.wrapping_add((b0 as u32) << 9);
        self.low = self.low.wrapping_add((b1 as u32) << 1);
        self.low = self.low.wrapping_sub(CABAC_MASK);
        self.pos += 2;
    }

    /// Decode a context-coded bin (HEVC spec 9.3.4.3.2). Returns 0 or 1 and
    /// updates the context state.
    #[inline]
    pub fn decode_bin(&mut self, state: &mut u8) -> u32 {
        let s = *state;
        // Bits 6..7 of the (normalised, 9-bit) range pick one of the four
        // rangeTabLps columns (spec table 9-46).
        let fused = CABAC_FUSED[((s as usize) << 2) | ((self.range >> 6) & 3) as usize];
        let range_lps = fused & 0xff;

        self.range -= range_lps;
        // FFmpeg-style branchless MPS/LPS resolution: build a sign-extended
        // mask that's 0 for MPS and 0xFFFFFFFF for LPS.
        let lps_mask =
            (((self.range << (CABAC_BITS + 1)).wrapping_sub(self.low)) as i32 >> 31) as u32;

        self.low = self
            .low
            .wrapping_sub((self.range << (CABAC_BITS + 1)) & lps_mask);
        self.range = self
            .range
            .wrapping_add(range_lps.wrapping_sub(self.range) & lps_mask);

        // Next state: bits 8..16 after an MPS, 16..24 after an LPS.
        *state = (fused >> (8 + (lps_mask & 8))) as u8;
        // The decoded bin is valMps, inverted for an LPS.
        let bit = ((s as u32) ^ lps_mask) & 1;

        // Renormalization (spec 9.3.4.3.5).
        let shift = norm_shift(self.range);
        self.range <<= shift;
        self.low = self.low.wrapping_shl(shift);
        if self.low & CABAC_MASK == 0 {
            self.refill2();
        }
        bit
    }

    /// Decode a bypass bin (HEVC spec 9.3.4.3.4). Equiprobable, no context.
    #[inline]
    pub fn decode_bypass(&mut self) -> u32 {
        self.low = self.low.wrapping_add(self.low);
        if self.low & CABAC_MASK == 0 {
            self.refill();
        }
        let range = self.range << (CABAC_BITS + 1);
        if self.low < range {
            0
        } else {
            self.low = self.low.wrapping_sub(range);
            1
        }
    }

    /// Alignment prior to aligned bypass decoding (spec 9.3.4.3.6, used with
    /// cabac_bypass_alignment_enabled_flag): ivlCurrRange = 256, so the
    /// following bypass bins read the bitstream one bit at a time.
    pub fn align_bypass(&mut self) {
        self.range = 256;
    }

    /// Decode `n` bypass bins as an unsigned integer (MSB first).
    pub fn decode_bypass_bits(&mut self, n: u8) -> u32 {
        let mut val = 0u32;
        for _ in 0..n {
            val = (val << 1) | self.decode_bypass();
        }
        val
    }

    /// Decode the end-of-slice flag (HEVC spec 9.3.4.3.5).
    /// Returns 1 if the slice is terminating, 0 otherwise.
    pub fn decode_terminate(&mut self) -> u32 {
        self.range -= 2;
        if self.low < self.range << (CABAC_BITS + 1) {
            // Renormalize once
            let shift = (self.range.wrapping_sub(0x100)) >> 31;
            self.range <<= shift;
            self.low = self.low.wrapping_shl(shift);
            if self.low & CABAC_MASK == 0 {
                self.refill();
            }
            0
        } else {
            1
        }
    }

    /// Current byte position in the RBSP (used for diagnostics).
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Borrow the underlying RBSP byte slice (used by PCM sample decoding
    /// after `pcm_byte_position` identifies where the raw bytes start).
    pub fn rbsp(&self) -> &[u8] {
        self.data
    }

    /// After decoding a `pcm_flag = 1` terminate bin, compute the byte offset
    /// in the underlying RBSP where the first raw PCM byte lives. Mirrors
    /// FFmpeg's `skip_bytes` (`libavcodec/cabac_functions.h`) which walks the
    /// `bytestream` pointer back by up to two bytes based on the current
    /// renormalization state (`low & 0x1` / `low & 0x1FF`).
    ///
    /// Background: `self.pos` is the byte CABAC would next refill from. But
    /// because of the 16-bit buffered window and the post-decode
    /// renormalization, anywhere from zero to two of those "already-consumed"
    /// bytes are still sitting in `low` unused when the terminate bin fires.
    /// Those unused bytes become the PCM byte stream; subtracting them from
    /// `self.pos` gives the address of the first PCM byte.
    pub fn pcm_byte_position(&self) -> usize {
        let mut ptr = self.pos;
        if self.low & 0x1 != 0 {
            ptr -= 1;
        }
        // CABAC_BITS == 16 in our implementation, so the second check always
        // applies (see FFmpeg's `#if CABAC_BITS == 16`).
        if self.low & 0x1FF != 0 {
            ptr -= 1;
        }
        ptr
    }

    /// Re-initialize the CABAC engine at a new byte offset. Used after the
    /// PCM block's raw bytes have been consumed, to resume CABAC decoding at
    /// the next byte boundary (HEVC spec 7.3.8.5 / FFmpeg `ff_init_cabac_decoder`).
    ///
    /// Returns `Err` if `byte_offset` is too close to the end of the RBSP
    /// to read the 2 seed bytes (malformed bitstream).
    pub fn reinit_at(&mut self, byte_offset: usize) -> Result<(), crate::error::DecodeError> {
        if byte_offset + 2 > self.data.len() {
            return Err(crate::error::DecodeError::InvalidSyntax(
                "CABAC reinit byte offset past end of RBSP",
            ));
        }
        let mut low: u32 = (self.data[byte_offset] as u32) << 18;
        low = low.wrapping_add((self.data[byte_offset + 1] as u32) << 10);
        low = low.wrapping_add(1 << 9);
        self.low = low;
        self.range = 0x1FE;
        self.pos = byte_offset + 2;
        Ok(())
    }
}

/// Initialize a single CABAC context from a HEVC packed init value
/// (spec 9.3.4.2.1).
///
/// The HEVC init value packs two 4-bit indices: the high nibble is `slopeIdx`
/// and the low nibble is `offsetIdx`. From these we derive `m` and `n` and
/// then compute `preCtxState`, exactly as in H.264 — the only difference is
/// the packing convention.
///
/// Returns the FFmpeg-style packed state value (`2 * pStateIdx + valMps`)
/// expected by [`CabacReader::decode_bin`].
pub fn init_state(init_value: u8, slice_qp: i32) -> u8 {
    let slope_idx = (init_value >> 4) as i32;
    let offset_idx = (init_value & 0x0F) as i32;
    let m = slope_idx * 5 - 45;
    let n = (offset_idx << 3) - 16;
    let qp = slice_qp.clamp(0, 51);

    // Same trick as rust_h264: compute `2 * preCtxState - 127`, fold the sign
    // away, and clamp into [0, 125]. The mapping is:
    //   preCtxState <= 63: pStateIdx = 63 - pre, valMps = 0
    //                       → packed = 2*(63 - pre) + 0 = 126 - 2*pre
    //   preCtxState >= 64: pStateIdx = pre - 64, valMps = 1
    //                       → packed = 2*(pre - 64) + 1 = 2*pre - 127
    // Both cases collapse to `|2*pre - 127|`.
    let mut pre = 2 * (((m * qp) >> 4) + n) - 127;
    pre ^= pre >> 31;
    if pre > 124 {
        pre = 124 + (pre & 1);
    }
    pre as u8
}

/// Initialize a slice of contexts in one shot. `init_values[i]` corresponds
/// to `state[i]`.
pub fn init_states(init_values: &[u8], slice_qp: i32, state: &mut [u8]) {
    debug_assert_eq!(init_values.len(), state.len());
    for (i, &iv) in init_values.iter().enumerate() {
        state[i] = init_state(iv, slice_qp);
    }
}

/// Compute the HEVC `initType` index into [`INIT_VALUES`] for the given
/// slice type and `cabac_init_flag` (spec 9.3.2.2 / FFmpeg
/// `cabac_init_state`).
///
/// Mapping:
/// - I slice → 0 (`cabac_init_flag` is not present in I slices, ignored)
/// - P slice → 1, or 2 if `cabac_init_flag` is set
/// - B slice → 2, or 1 if `cabac_init_flag` is set
pub fn init_type_for_slice(slice_type: SliceType, cabac_init_flag: bool) -> usize {
    let base = 2 - slice_type as i32;
    let it = if cabac_init_flag && slice_type != SliceType::I {
        base ^ 3
    } else {
        base
    };
    it as usize
}

/// Full set of HEVC CABAC contexts for one slice. Layout matches FFmpeg's
/// `cabac_state[HEVC_CONTEXTS]` exactly so the offsets in
/// [`crate::cabac_tables::ctx`] index directly into [`Self::state`].
#[derive(Clone, Copy)]
pub struct CabacContexts {
    pub state: [u8; HEVC_CONTEXTS],
    /// StatCoeff[sbType] for persistent Rice adaptation (spec 9.3.3.11).
    /// Reset with the contexts and synchronised with them (WPP, dependent
    /// slice segments).
    pub stat_coeff: [u8; 4],
}

impl CabacContexts {
    /// Initialize all 179 contexts from the slice QP, slice type and
    /// `cabac_init_flag` (HEVC spec 9.3.2.2).
    pub fn init(slice_qp: i32, slice_type: SliceType, cabac_init_flag: bool) -> Self {
        let init_type = init_type_for_slice(slice_type, cabac_init_flag);
        let row = &INIT_VALUES[init_type];
        let mut state = [0u8; HEVC_CONTEXTS];
        for i in 0..HEVC_CONTEXTS {
            state[i] = init_state(row[i], slice_qp);
        }
        Self {
            state,
            stat_coeff: [0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `norm_shift` (leading zeros) equals the shift that brings each
    /// possible range back to at least 256.
    #[test]
    fn norm_shift_matches_reference_loop() {
        for range in 1u32..512 {
            let mut r = range;
            let mut shift = 0;
            while r < 256 {
                r <<= 1;
                shift += 1;
            }
            assert_eq!(norm_shift(range), shift, "range {range}");
        }
    }

    /// Every fused entry equals the separate rangeTabLps / transition tables.
    #[test]
    fn fused_table_matches_source_tables() {
        use crate::cabac_tables::{LPS_RANGE, MLPS_STATE};
        for s in 0usize..128 {
            for q in 0usize..4 {
                let e = CABAC_FUSED[(s << 2) | q];
                assert_eq!(e & 0xff, LPS_RANGE[q * 128 + s] as u32, "lps s{s} q{q}");
                assert_eq!((e >> 8) & 0xff, MLPS_STATE[128 + s] as u32, "mps s{s}");
                assert_eq!(
                    (e >> 16) & 0xff,
                    MLPS_STATE[127 - s] as u32,
                    "lps-next s{s}"
                );
            }
        }
    }

    /// Hand-decoded reference: a CABAC bypass-only stream of 8 bits with the
    /// MSB-first value `0b10110100 = 0xB4`. Encoded as the trivial range-coded
    /// representation (8 bypass bins). We use this to verify the bypass path
    /// without depending on context tables.
    ///
    /// To construct the byte stream we leverage the fact that bypass bins are
    /// equivalent to bit reads — the engine just shifts `low` left by one and
    /// compares against `range << (CABAC_BITS + 1)`. With `range == 510`
    /// throughout (no context updates), each bypass bin maps directly to a
    /// bit of the input. So a stream that emits the MSB of `0xB4` first must
    /// start with byte `0xB4`.
    #[test]
    fn test_decode_bypass_round_trip() {
        // 16 bytes is well above CABAC's 2-byte init footprint and avoids any
        // padding edge cases.
        let data = [0xB4, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut cabac = CabacReader::new(&data, 0).unwrap();
        // After reading 0xB4 = 0b1011_0100 we should observe these bits in order.
        let expected = [1, 0, 1, 1, 0, 1, 0, 0];
        for &b in &expected {
            assert_eq!(cabac.decode_bypass(), b, "bypass bin mismatch");
        }
    }

    /// Decoding `n` bypass bits should equal the multi-bit helper.
    #[test]
    fn test_decode_bypass_bits_matches_loop() {
        let data = [0xCA, 0xFE, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        let mut a = CabacReader::new(&data, 0).unwrap();
        let mut b = CabacReader::new(&data, 0).unwrap();
        let n = 12;
        let v1 = a.decode_bypass_bits(n);
        let mut v2 = 0u32;
        for _ in 0..n {
            v2 = (v2 << 1) | b.decode_bypass();
        }
        assert_eq!(v1, v2);
        assert_eq!(v1 >> (32 - n), 0); // top bits zero
    }

    /// `init_state` clamps QP to [0, 51] and produces a state in [0, 125].
    #[test]
    fn test_init_state_in_range() {
        for &iv in &[0u8, 0x10, 0x77, 0x88, 0xC9, 0xFE, 0xFF] {
            for &qp in &[-10i32, 0, 1, 26, 51, 60] {
                let s = init_state(iv, qp);
                assert!(s <= 125, "state {} out of range for iv={} qp={}", s, iv, qp);
            }
        }
    }

    /// Hand-computed initial states for the Phase 1 fixture's slice QP (25)
    /// and a handful of context offsets. Each derivation is in a comment for
    /// auditability — if any of these break, the table or formula is wrong.
    #[test]
    fn test_init_known_states_at_qp25() {
        use crate::cabac_tables::ctx;
        let ctxs = CabacContexts::init(25, SliceType::I, false);

        // split_cu_flag[0] (init_value = 139): slope=8 offset=11
        //   m = -5, n = 72
        //   pre_inner = (-5 * 25) >> 4 = -8;  pre_inner + n = 64
        //   2*64 - 127 = 1;  state = 1
        assert_eq!(ctxs.state[ctx::SPLIT_CODING_UNIT_FLAG], 1);

        // prev_intra_luma_pred_flag (init_value = 184): slope=11 offset=8
        //   m = 10, n = 48
        //   pre_inner = (10 * 25) >> 4 = 15;  pre_inner + n = 63
        //   2*63 - 127 = -1; |.| = 0;  state = 0
        assert_eq!(ctxs.state[ctx::PREV_INTRA_LUMA_PRED_FLAG], 0);

        // cbf_luma[0] (init_value = 111): slope=6 offset=15
        //   m = -15, n = 104
        //   pre_inner = (-15 * 25) >> 4 = -24;  pre_inner + n = 80
        //   2*80 - 127 = 33;  state = 33
        assert_eq!(ctxs.state[ctx::CBF_LUMA], 33);

        // intra_chroma_pred_mode[0] (init_value = 63): slope=3 offset=15
        //   m = -30, n = 104
        //   pre_inner = (-30 * 25) >> 4 = -47;  pre_inner + n = 57
        //   2*57 - 127 = -13;  state = 12
        assert_eq!(ctxs.state[ctx::INTRA_CHROMA_PRED_MODE], 12);
    }

    /// `init_type_for_slice` mapping must match HEVC spec 9.3.2.2.
    #[test]
    fn test_init_type_for_slice() {
        // I-slice ignores cabac_init_flag and always picks 0.
        assert_eq!(init_type_for_slice(SliceType::I, false), 0);
        assert_eq!(init_type_for_slice(SliceType::I, true), 0);
        // P-slice: 1 by default, 2 with cabac_init_flag.
        assert_eq!(init_type_for_slice(SliceType::P, false), 1);
        assert_eq!(init_type_for_slice(SliceType::P, true), 2);
        // B-slice: 2 by default, 1 with cabac_init_flag.
        assert_eq!(init_type_for_slice(SliceType::B, false), 2);
        assert_eq!(init_type_for_slice(SliceType::B, true), 1);
    }

    /// `init_state` matches the explicit two-step HEVC formula.
    #[test]
    fn test_init_state_matches_spec_formula() {
        // Recreate (pStateIdx, valMps) by hand and compare to packed encoding.
        for iv in 0u8..=255 {
            for &qp in &[0, 13, 26, 37, 51] {
                let slope_idx = (iv >> 4) as i32;
                let offset_idx = (iv & 0x0F) as i32;
                let m = slope_idx * 5 - 45;
                let n = (offset_idx << 3) - 16;
                let pre = (((m * qp) >> 4) + n).clamp(1, 126);
                let (p_state_idx, val_mps): (i32, i32) = if pre <= 63 {
                    (63 - pre, 0)
                } else {
                    (pre - 64, 1)
                };
                let expected_packed = (2 * p_state_idx + val_mps) as u8;
                // The packed encoding uses the same formula but via the
                // FFmpeg `|2*pre - 127|` trick. Verify equivalence.
                let actual = init_state(iv, qp);
                assert_eq!(
                    actual, expected_packed,
                    "init_state(iv={iv:#04x}, qp={qp}) packed mismatch: got {actual}, want {expected_packed}",
                );
            }
        }
    }

    /// `reinit_at` should reset the arithmetic engine to a fresh state,
    /// equivalent to constructing a new `CabacReader` at the same offset.
    #[test]
    fn test_reinit_at_matches_fresh_new() {
        let data = [0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x23, 0x45, 0x67];
        let mut a = CabacReader::new(&data, 0).unwrap();
        // Consume a few bypass bins to advance state.
        let _ = a.decode_bypass_bits(4);
        a.reinit_at(4).unwrap();
        let b = CabacReader::new(&data, 4).unwrap();
        assert_eq!(a.low, b.low);
        assert_eq!(a.range, b.range);
        assert_eq!(a.pos, b.pos);
    }
}
