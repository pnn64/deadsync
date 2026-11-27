//! Parsing of the uncompressed frame header (`uncompressed_header`) (spec §6.2, §7.2).
//!
//! The uncompressed frame header is not parsed by the bool decoder but by
//! `BitReader`'s plain MSB-first bit reading (`f(n)` /
//! `s(n)` descriptors, spec §9.1).
//!
//! M1/M2 only supported key frames, but M3 added support for parsing inter
//! frames and intra-only frames as well (the entirety of `uncompressed_header()`
//! in spec §6.2). Since `frame_size_with_refs()` (spec §6.2.5) requires the
//! reference frame slot sizes (`RefFrameWidth`/`RefFrameHeight`),
//! [`parse_uncompressed_header`] is designed to receive them from the caller
//! (the caller holds the cross-frame state).

use core::fmt;

use crate::bit_reader::BitReader;
// The values of `ref_frame`/`interpolation_filter` and the number of reference
// frame slots are shared across multiple modules (motion vector prediction in
// tile.rs, frame_reference_mode in compressed_header.rs, etc.), so they are
// defined once in `prob_tables`.
pub use crate::prob_tables::{
    BILINEAR, EIGHTTAP, EIGHTTAP_SHARP, EIGHTTAP_SMOOTH, LAST_FRAME, NUM_REF_FRAMES, SWITCHABLE,
};

const LITERAL_TO_TYPE: [u8; 4] = [EIGHTTAP_SMOOTH, EIGHTTAP, EIGHTTAP_SHARP, BILINEAR];

/// Value of the `frame_type` syntax element (spec §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// frame_type == 0
    KeyFrame,
    /// frame_type == 1
    NonKeyFrame,
}

/// Known values of `color_space` (table in spec §7.2.2).
pub const CS_UNKNOWN: u8 = 0;
pub const CS_BT_601: u8 = 1;
pub const CS_RGB: u8 = 7;

/// Errors that can occur while parsing the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    /// `frame_marker` was not the spec-mandated value of 2.
    InvalidFrameMarker,
    /// `frame_sync_code` was not the spec-mandated 0x49 0x83 0x42.
    InvalidSyncCode,
    /// `color_space == CS_RGB` and `profile_low_bit == 0`
    /// (violates a spec conformance requirement: RGB is not usable in profiles 0 and 2).
    InvalidColorConfigForProfile,
    /// `FrameWidth * FrameHeight` exceeds the decoder's configured allocation limits
    /// (by default `1 << 27` luma samples) -- a resource-limit rejection (not a spec
    /// constraint: the bitstream's 16-bit dimensions permit up to 65536x65536, whose frame buffer
    /// cannot be allocated). Guards against a malformed header's absurd size aborting the process
    /// on the allocation (an out-of-memory DoS).
    FrameSizeTooLarge,
    /// Frame dimensions are zero (e.g. `frame_size_with_refs` signaled `found_ref = 1` for a DPB
    /// slot that has never been written).
    InvalidFrameSize,
    /// An inter frame's reference has a different bit depth or chroma subsampling than the
    /// current frame (spec §8.5.1 / libvpx `valid_ref_frame_img_fmt`: motion compensation
    /// across mismatched sample formats is not permitted). A malformed stream mixing e.g.
    /// 4:4:4 and 4:2:0 frames is rejected here instead of silently mis-predicting.
    RefFrameFormatMismatch,
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderError::InvalidFrameMarker => f.write_str("invalid VP9 frame marker (expected 2)"),
            HeaderError::InvalidSyncCode => f.write_str("invalid VP9 sync code"),
            HeaderError::InvalidColorConfigForProfile => {
                f.write_str("RGB color space is not permitted for VP9 profile 0 or 2")
            }
            HeaderError::FrameSizeTooLarge => {
                f.write_str("frame dimensions exceed decoder allocation limit")
            }
            HeaderError::InvalidFrameSize => {
                f.write_str("frame dimensions are zero or reference an uninitialized DPB slot")
            }
            HeaderError::RefFrameFormatMismatch => {
                f.write_str("reference frame bit depth or subsampling does not match current frame")
            }
        }
    }
}

impl core::error::Error for HeaderError {}

/// Upper bound on `FrameWidth * FrameHeight` (luma samples) this decoder will allocate a frame
/// buffer for; larger headers are rejected as [`HeaderError::FrameSizeTooLarge`]. Chosen to sit
/// far above any real content (8K UHD is ~33M samples) yet far below the bitstream's pathological
/// 65536x65536 (~4.3G) maximum, bounding worst-case allocation. Not a spec limit -- see the
/// variant's doc.
pub const MAX_FRAME_LUMA_SAMPLES: u64 = 1 << 27;

/// Loop-filter-related parameters (spec §6.2.8 `loop_filter_params`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopFilterParams {
    pub level: u8,
    pub sharpness: u8,
    pub delta_enabled: bool,
    /// Adjustment value per reference frame type. Indexed in the order
    /// `[INTRA_FRAME, LAST_FRAME, GOLDEN_FRAME, ALTREF_FRAME]`.
    /// For key frames, `setup_past_independence()` initializes this to
    /// `[1, 0, -1, -1]` (spec §7.2).
    pub ref_deltas: [i8; 4],
    pub mode_deltas: [i8; 2],
}

/// Quantization parameters (spec §6.2.9 `quantization_params`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantizationParams {
    pub base_q_idx: u8,
    pub delta_q_y_dc: i32,
    pub delta_q_uv_dc: i32,
    pub delta_q_uv_ac: i32,
    /// `Lossless = base_q_idx == 0 && delta_q_y_dc == 0 && delta_q_uv_dc == 0 && delta_q_uv_ac == 0`
    pub lossless: bool,
}

/// Color config (spec §6.2.2 `color_config`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorConfig {
    pub bit_depth: u8,
    pub color_space: u8,
    pub color_range: bool,
    pub subsampling_x: u8,
    pub subsampling_y: u8,
}

impl ColorConfig {
    /// Spec §6.2.2 default color config for `intra_only` frames in profile 0 (8-bit, BT.601,
    /// studio range, 4:2:0).
    pub(crate) const fn default_intra_only_profile0() -> Self {
        Self {
            bit_depth: 8,
            color_space: CS_BT_601,
            color_range: false,
            subsampling_x: 1,
            subsampling_y: 1,
        }
    }

    /// Fallback color config when a non-intra frame arrives before any key or intra-only frame.
    pub(crate) const fn fallback_inter() -> Self {
        Self {
            bit_depth: 8,
            color_space: CS_UNKNOWN,
            color_range: false,
            subsampling_x: 1,
            subsampling_y: 1,
        }
    }
}

/// The loop filter's `ref_deltas`/`mode_deltas` (spec §7.2), persisted across frames by the
/// caller and reset only by `setup_past_independence()`. Replaces the former
/// `([i8; 4], [i8; 2])` tuple threaded through [`parse_uncompressed_header`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopFilterDeltas {
    pub ref_deltas: [i8; 4],
    pub mode_deltas: [i8; 2],
}

impl Default for LoopFilterDeltas {
    fn default() -> Self {
        Self {
            ref_deltas: DEFAULT_LOOP_FILTER_REF_DELTAS,
            mode_deltas: DEFAULT_LOOP_FILTER_MODE_DELTAS,
        }
    }
}

/// The segmentation `FeatureEnabled`/`FeatureData`/`segmentation_abs_or_delta_update` state
/// (spec §7.2.10), persisted across frames by the caller and reset only by
/// `setup_past_independence()`. Replaces the former `(SegFeatureEnabled, SegFeatureData,
/// bool)` tuple threaded through [`parse_uncompressed_header`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SegFeaturePersist {
    pub enabled: SegFeatureEnabled,
    pub data: SegFeatureData,
    pub abs_or_delta: bool,
}

/// All of the cross-frame state [`parse_uncompressed_header`] needs from the caller (spec
/// §7.2): the reference frame slot sizes (`RefFrameWidth`/`RefFrameHeight`, used by
/// `frame_size_with_refs()`, spec §6.2.5), and the loop filter/segmentation state that
/// `setup_past_independence()` would otherwise reset. Callers (`Decoder`) keep one instance of
/// this across frames instead of three separate fields.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PersistentState {
    pub ref_frame_sizes: [(u32, u32); NUM_REF_FRAMES],
    pub loop_filter_deltas: LoopFilterDeltas,
    pub segmentation: SegFeaturePersist,
}

/// A parsed uncompressed frame header.
// `New` is the overwhelmingly common path and is consumed immediately by the decoder; boxing it
// would add one heap allocation to every newly decoded frame merely to shrink this transient enum.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameHeader {
    /// `show_existing_frame == 1`. No new decode is performed; the frame at the given index is displayed.
    ShowExistingFrame { frame_to_show_map_idx: u8 },
    /// A newly decoded frame with the remaining uncompressed-header fields.
    New(NewFrameHeader),
}

/// The full set of uncompressed header fields for a newly decoded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFrameHeader {
    pub profile: u8,
    pub frame_type: FrameType,
    pub show_frame: bool,
    pub error_resilient_mode: bool,
    /// `FrameIsIntra`. Always true for key frames. For non-key frames, equal to `intra_only`.
    pub frame_is_intra: bool,
    /// `intra_only`. Read from the bitstream only when `frame_type == NonKeyFrame`
    /// and `show_frame == 0` (otherwise 0).
    pub intra_only: bool,
    /// `reset_frame_context` (spec §7.2). Always 0 when `error_resilient_mode == 1`.
    pub reset_frame_context: u8,
    /// `Some` exactly when `color_config()` is actually parsed or spec-defined for this frame:
    /// key frames (always), and `intra_only` frames (parsed when `profile > 0`, or the
    /// spec-defined 8-bit/CS_BT_601/4:2:0 default when `profile == 0`, spec §6.2). `None` for
    /// a regular inter frame, which per spec §7.2 doesn't resend `color_config` at all (it's a
    /// conformance requirement that it match the reference frame) — the caller (`Decoder`)
    /// resolves this from the most recently seen key/intra-only frame's value.
    pub color_config: Option<ColorConfig>,
    pub width: u32,
    pub height: u32,
    pub render_width: u32,
    pub render_height: u32,
    /// Bitmask of reference frame slots to update. Always 0xFF for key frames.
    pub refresh_frame_flags: u8,
    /// Frame slot numbers referenced for inter prediction (in the order
    /// `LAST_FRAME`/`GOLDEN_FRAME`/`ALTREF_FRAME`). Meaningless when
    /// `FrameIsIntra == 1` (`[0, 0, 0]`).
    pub ref_frame_idx: [u8; 3],
    /// `ref_frame_sign_bias[ i ]`. The index has the same semantics as the
    /// `ref_frame` value (`INTRA_FRAME`..`ALTREF_FRAME`, i.e. 0..3). Always
    /// `[false; 4]` when `FrameIsIntra == 1` (via `setup_past_independence()`).
    pub ref_frame_sign_bias: [bool; 4],
    /// `allow_high_precision_mv`. Meaningless (`false`) when `FrameIsIntra == 1`
    /// or for intra-only frames.
    pub allow_high_precision_mv: bool,
    /// `interpolation_filter` (a value in `EIGHTTAP`..`SWITCHABLE`).
    pub interpolation_filter: u8,
    pub refresh_frame_context: bool,
    pub frame_parallel_decoding_mode: bool,
    /// `frame_context_idx` as read from the bitstream (`f(2)`, spec §7.1.2) — the raw value,
    /// the single source of truth. The index actually used for `load_probs`/`save_probs` is
    /// [`Self::effective_frame_context_idx`], which forces this to 0 when
    /// `FrameIsIntra || error_resilient_mode` (`setup_past_independence()`, spec §7.2). The
    /// `save_probs` reset in the `reset_frame_context == 2` case (spec §7.2) targets this raw
    /// value directly (it runs before the forcing), so both consumers derive from this one field.
    pub frame_context_idx: u8,
    pub loop_filter: LoopFilterParams,
    pub quantization: QuantizationParams,
    pub segmentation: SegmentationParams,
    pub tile_cols_log2: u32,
    pub tile_rows_log2: u32,
    /// Size in bytes of the compressed header (`compressed_header`). The bool
    /// decoder starts right after this via `init_bool(header_size_in_bytes)`.
    pub header_size_in_bytes: u16,
}

impl NewFrameHeader {
    /// The `frame_context_idx` actually used for `load_probs`/`save_probs` (spec §7.1.2): the
    /// raw bitstream value ([`Self::frame_context_idx`]), forced to 0 when
    /// `FrameIsIntra || error_resilient_mode` — the reset `setup_past_independence()` applies
    /// (spec §7.2). Derived on read so the raw value stays the single source of truth.
    pub fn effective_frame_context_idx(&self) -> u8 {
        if self.frame_is_intra || self.error_resilient_mode {
            0
        } else {
            self.frame_context_idx
        }
    }
}

const MAX_TILE_WIDTH_B64: u32 = 64;
const MIN_TILE_WIDTH_B64: u32 = 4;

/// Values derived from the frame size, needed for computing the tile
/// partition count (spec §6.2.6 `compute_image_size`).
///
/// `mi_cols`/`mi_rows` are the frame width/height in 8x8 units (mode info
/// units); `sb64_cols`/`sb64_rows` are the frame width/height in 64x64 units
/// (superblock units). Made `pub(crate)` since it is also used for tile and
/// superblock traversal (`src/tile.rs`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImageSize {
    pub(crate) mi_cols: u32,
    pub(crate) mi_rows: u32,
    pub(crate) sb64_cols: u32,
    pub(crate) sb64_rows: u32,
}

pub(crate) fn compute_image_size(width: u32, height: u32) -> ImageSize {
    let mi_cols = (width + 7) >> 3;
    let mi_rows = (height + 7) >> 3;
    let sb64_cols = (mi_cols + 7) >> 3;
    let sb64_rows = (mi_rows + 7) >> 3;
    ImageSize {
        mi_cols,
        mi_rows,
        sb64_cols,
        sb64_rows,
    }
}

/// `calc_min_log2_tile_cols()` (spec §6.2.14).
fn calc_min_log2_tile_cols(sb64_cols: u32) -> u32 {
    let mut min_log2 = 0u32;
    while (MAX_TILE_WIDTH_B64 << min_log2) < sb64_cols {
        min_log2 += 1;
    }
    min_log2
}

/// `calc_max_log2_tile_cols()` (spec §6.2.14).
fn calc_max_log2_tile_cols(sb64_cols: u32) -> u32 {
    let mut max_log2 = 1u32;
    while (sb64_cols >> max_log2) >= MIN_TILE_WIDTH_B64 {
        max_log2 += 1;
    }
    max_log2 - 1
}

/// `read_prob()` (spec §6.2.12).
fn read_prob(r: &mut BitReader) -> u8 {
    if r.flag() {
        r.f(8) as u8
    } else {
        255
    }
}

/// `read_delta_q()` (spec §6.2.10).
fn read_delta_q(r: &mut BitReader) -> i32 {
    if r.flag() {
        r.s(4)
    } else {
        0
    }
}

/// `color_config()` (spec §6.2.2).
fn parse_color_config(r: &mut BitReader, profile: u8) -> Result<ColorConfig, HeaderError> {
    let bit_depth = if profile >= 2 {
        if r.flag() {
            12
        } else {
            10
        }
    } else {
        8
    };

    let color_space = r.f(3) as u8;

    // Spec conformance requirement: CS_RGB is not usable when profile_low_bit == 0 (Profile 0 or 2).
    if color_space == CS_RGB && profile & 1 == 0 {
        return Err(HeaderError::InvalidColorConfigForProfile);
    }

    let (color_range, subsampling_x, subsampling_y) = if color_space != CS_RGB {
        let color_range = r.flag();
        let (sx, sy) = if profile == 1 || profile == 3 {
            let sx = r.f(1) as u8;
            let sy = r.f(1) as u8;
            let _reserved_zero = r.f(1);
            (sx, sy)
        } else {
            (1u8, 1u8)
        };
        (color_range, sx, sy)
    } else {
        if profile == 1 || profile == 3 {
            let _reserved_zero = r.f(1);
        }
        (true, 0u8, 0u8)
    };

    Ok(ColorConfig {
        bit_depth,
        color_space,
        color_range,
        subsampling_x,
        subsampling_y,
    })
}

/// `frame_size()` + `compute_image_size()` (spec §6.2.3, §6.2.6).
fn parse_frame_size(r: &mut BitReader) -> (u32, u32) {
    let frame_width_minus_1 = r.f(16);
    let frame_height_minus_1 = r.f(16);
    (frame_width_minus_1 + 1, frame_height_minus_1 + 1)
}

/// `render_size()` (spec §6.2.4).
fn parse_render_size(r: &mut BitReader, width: u32, height: u32) -> (u32, u32) {
    if r.flag() {
        let render_width_minus_1 = r.f(16);
        let render_height_minus_1 = r.f(16);
        (render_width_minus_1 + 1, render_height_minus_1 + 1)
    } else {
        (width, height)
    }
}

/// `frame_size_with_refs()` (spec §6.2.5). If `found_ref == 1` for any of the
/// slots pointed to by `ref_frame_idx`, that slot's size (passed in externally
/// as `ref_frame_sizes`) is used directly as `FrameWidth`/`FrameHeight`. If
/// none is found, `frame_size()` is read instead. Rejects a `found_ref == 1`
/// pointing to a never-written `(0, 0)` DPB slot as [`HeaderError::InvalidFrameSize`].
fn parse_frame_size_with_refs(
    r: &mut BitReader,
    ref_frame_idx: [u8; 3],
    ref_frame_sizes: &[(u32, u32); NUM_REF_FRAMES],
) -> Result<(u32, u32), HeaderError> {
    for &idx in ref_frame_idx.iter() {
        if r.flag() {
            let (w, h) = ref_frame_sizes[idx as usize];
            if w == 0 || h == 0 {
                return Err(HeaderError::InvalidFrameSize);
            }
            return Ok((w, h));
        }
    }
    Ok(parse_frame_size(r))
}

/// `read_interpolation_filter()` (spec §6.2.7).
fn parse_interpolation_filter(r: &mut BitReader) -> u8 {
    let is_filter_switchable = r.flag();
    if is_filter_switchable {
        SWITCHABLE
    } else {
        let raw = r.f(2) as usize;
        LITERAL_TO_TYPE[raw]
    }
}

/// Initial loop filter delta values set by `setup_past_independence()` (spec §7.2).
pub const DEFAULT_LOOP_FILTER_REF_DELTAS: [i8; 4] = [1, 0, -1, -1];
pub const DEFAULT_LOOP_FILTER_MODE_DELTAS: [i8; 2] = [0, 0];

/// `loop_filter_params()` (spec §6.2.8).
///
/// `ref_deltas`/`mode_deltas` are, per the spec, state that persists across
/// frames (reset to default values only when `setup_past_independence()` is
/// called). The caller ([`parse_uncompressed_header`]) passes in the `reset`
/// flag (`FrameIsIntra || error_resilient_mode`) and the previous frame's values.
fn parse_loop_filter_params(
    r: &mut BitReader,
    reset: bool,
    prev_deltas: LoopFilterDeltas,
) -> LoopFilterParams {
    let level = r.f(6) as u8;
    let sharpness = r.f(3) as u8;
    let delta_enabled = r.flag();

    let (mut ref_deltas, mut mode_deltas) = if reset {
        (
            DEFAULT_LOOP_FILTER_REF_DELTAS,
            DEFAULT_LOOP_FILTER_MODE_DELTAS,
        )
    } else {
        (prev_deltas.ref_deltas, prev_deltas.mode_deltas)
    };

    if delta_enabled && r.flag() {
        for delta in ref_deltas.iter_mut().chain(mode_deltas.iter_mut()) {
            if r.flag() {
                *delta = r.s(6) as i8;
            }
        }
    }

    LoopFilterParams {
        level,
        sharpness,
        delta_enabled,
        ref_deltas,
        mode_deltas,
    }
}

/// `quantization_params()` (spec §6.2.9).
fn parse_quantization_params(r: &mut BitReader) -> QuantizationParams {
    let base_q_idx = r.f(8) as u8;
    let delta_q_y_dc = read_delta_q(r);
    let delta_q_uv_dc = read_delta_q(r);
    let delta_q_uv_ac = read_delta_q(r);
    let lossless = base_q_idx == 0 && delta_q_y_dc == 0 && delta_q_uv_dc == 0 && delta_q_uv_ac == 0;

    QuantizationParams {
        base_q_idx,
        delta_q_y_dc,
        delta_q_uv_dc,
        delta_q_uv_ac,
        lossless,
    }
}

/// Segment feature level indices (`SEG_LVL_*`, spec §6.2.11 / §6.4.9 `seg_feature_active`).
pub const SEG_LVL_ALT_Q: usize = 0;
pub const SEG_LVL_ALT_L: usize = 1;
pub const SEG_LVL_REF_FRAME: usize = 2;
pub const SEG_LVL_SKIP: usize = 3;
pub const SEG_LVL_MAX: usize = 4;
/// Re-exported so existing `header::MAX_SEGMENTS` import paths (incl. `tests/`) keep working
/// now that the canonical definition lives in `common` (shared with `loop_filter.rs`, which
/// used to redefine it privately).
pub use crate::common::MAX_SEGMENTS;
const SEGMENTATION_FEATURE_BITS: [u32; SEG_LVL_MAX] = [8, 6, 2, 0];
const SEGMENTATION_FEATURE_SIGNED: [bool; SEG_LVL_MAX] = [true, true, false, false];

/// `FeatureEnabled[8][4]` (bool) / `FeatureData[8][4]` (i32) type aliases, shared
/// between [`SegmentationParams`] and the `prev_features` state threaded across
/// frames the same way `prev_loop_filter_deltas` is (see [`parse_uncompressed_header`]).
pub type SegFeatureEnabled = [[bool; SEG_LVL_MAX]; MAX_SEGMENTS];
pub type SegFeatureData = [[i32; SEG_LVL_MAX]; MAX_SEGMENTS];

/// Segmentation parameters (spec §6.2.11 `segmentation_params`).
///
/// Per spec §7.2.10, `feature_enabled`/`feature_data`/`abs_or_delta_update` are
/// state that persists across frames when not re-signaled in the bitstream
/// (`segmentation_update_data == 0`), reset to all-zero/false only by
/// `setup_past_independence()` (spec §7.2, `FrameIsIntra || error_resilient_mode`) —
/// the same persistence pattern as `LoopFilterParams::ref_deltas`/`mode_deltas`.
/// `tree_probs`/`pred_prob` need no such persistence: they are only read (and
/// only meaningful) within the same frame that has `update_map`/`temporal_update`
/// set, so they carry no state across frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentationParams {
    pub enabled: bool,
    /// `segmentation_update_map`. Only meaningful when `enabled`.
    pub update_map: bool,
    /// `segmentation_tree_probs[7]`. Only meaningful when `update_map`.
    pub tree_probs: [u8; 7],
    /// `segmentation_pred_prob[3]`. Only meaningful when `update_map && temporal_update`.
    pub pred_prob: [u8; 3],
    /// `segmentation_temporal_update`. Only meaningful when `update_map`.
    pub temporal_update: bool,
    pub abs_or_delta_update: bool,
    /// `FeatureEnabled[ segment ][ level ]`, `level` indexed by `SEG_LVL_*`.
    pub feature_enabled: SegFeatureEnabled,
    /// `FeatureData[ segment ][ level ]`.
    pub feature_data: SegFeatureData,
}

impl Default for SegmentationParams {
    /// A disabled segmentation configuration with neutral "not coded" probabilities and no
    /// active features. This is the initial/test-fixture state; frame parsing still preserves
    /// feature state explicitly when the bitstream does not update it.
    fn default() -> Self {
        Self {
            enabled: false,
            update_map: false,
            tree_probs: [255; 7],
            pred_prob: [255; 3],
            temporal_update: false,
            abs_or_delta_update: false,
            feature_enabled: [[false; SEG_LVL_MAX]; MAX_SEGMENTS],
            feature_data: [[0; SEG_LVL_MAX]; MAX_SEGMENTS],
        }
    }
}

/// `segmentation_params()` (spec §6.2.11). `reset`/`prev_features` mirror
/// [`parse_loop_filter_params`]'s `reset`/`prev_deltas`: `reset` is
/// `FrameIsIntra || error_resilient_mode` (the `setup_past_independence()`
/// condition, spec §7.2), under which `feature_enabled`/`feature_data`/
/// `abs_or_delta_update` start from all-zero/false instead of the previous frame's values.
fn parse_segmentation_params(
    r: &mut BitReader,
    reset: bool,
    prev_features: SegFeaturePersist,
) -> SegmentationParams {
    let enabled = r.flag();
    let (mut feature_enabled, mut feature_data, mut abs_or_delta_update) = if reset {
        (
            [[false; SEG_LVL_MAX]; MAX_SEGMENTS],
            [[0i32; SEG_LVL_MAX]; MAX_SEGMENTS],
            false,
        )
    } else {
        (
            prev_features.enabled,
            prev_features.data,
            prev_features.abs_or_delta,
        )
    };

    if !enabled {
        return SegmentationParams {
            abs_or_delta_update,
            feature_enabled,
            feature_data,
            ..SegmentationParams::default()
        };
    }

    let update_map = r.flag();
    let mut tree_probs = [255u8; 7];
    let mut pred_prob = [255u8; 3];
    let mut temporal_update = false;
    if update_map {
        for p in tree_probs.iter_mut() {
            *p = read_prob(r);
        }
        temporal_update = r.flag();
        for p in pred_prob.iter_mut() {
            // When temporal_update == 0, prob = 255 and no bit is consumed.
            *p = if temporal_update { read_prob(r) } else { 255 };
        }
    }

    let update_data = r.flag();
    if update_data {
        abs_or_delta_update = r.flag();
        for seg in feature_enabled.iter_mut().zip(feature_data.iter_mut()) {
            let (seg_enabled, seg_data) = seg;
            for level in 0..SEG_LVL_MAX {
                let enabled_bit = r.flag();
                seg_enabled[level] = enabled_bit;
                let mut feature_value = 0i32;
                if enabled_bit {
                    let bits_to_read = SEGMENTATION_FEATURE_BITS[level];
                    if bits_to_read > 0 {
                        feature_value = r.f(bits_to_read) as i32;
                    }
                    if SEGMENTATION_FEATURE_SIGNED[level] && r.flag() {
                        feature_value = -feature_value;
                    }
                }
                seg_data[level] = feature_value;
            }
        }
    }

    SegmentationParams {
        enabled,
        update_map,
        tree_probs,
        pred_prob,
        temporal_update,
        abs_or_delta_update,
        feature_enabled,
        feature_data,
    }
}

/// `tile_info()` (spec §6.2.13). Returns `(tile_cols_log2, tile_rows_log2)`.
fn parse_tile_info(r: &mut BitReader, sb64_cols: u32) -> (u32, u32) {
    let min_log2_tile_cols = calc_min_log2_tile_cols(sb64_cols);
    let max_log2_tile_cols = calc_max_log2_tile_cols(sb64_cols);

    let mut tile_cols_log2 = min_log2_tile_cols;
    while tile_cols_log2 < max_log2_tile_cols {
        if r.flag() {
            tile_cols_log2 += 1;
        } else {
            break;
        }
    }

    let mut tile_rows_log2 = r.f(1);
    if tile_rows_log2 == 1 {
        let increment_tile_rows_log2 = r.f(1);
        tile_rows_log2 += increment_tile_rows_log2;
    }

    (tile_cols_log2, tile_rows_log2)
}

/// Spec §6.2.1 `frame_sync_code()`: the 3-byte magic sequence `0x49 0x83 0x42`.
const VP9_SYNC_CODE: [u32; 3] = [0x49, 0x83, 0x42];

fn verify_sync_code(r: &mut BitReader) -> Result<(), HeaderError> {
    let sync = [r.f(8), r.f(8), r.f(8)];
    if sync != VP9_SYNC_CODE {
        return Err(HeaderError::InvalidSyncCode);
    }
    Ok(())
}

/// Parses `uncompressed_header()` (spec §6.2).
///
/// `prev` carries the cross-frame state spec §7.2 requires (see [`PersistentState`]):
/// the per-slot reference frame sizes (used by `frame_size_with_refs()`, spec §6.2.5; not
/// referenced for key frames or intra-only frames), and the loop filter/segmentation state
/// that `setup_past_independence()` would otherwise reset.
///
/// Returns a pair of the parse result and the number of bytes consumed,
/// including byte-boundary alignment via `trailing_bits()`.
pub fn parse_uncompressed_header(
    data: &[u8],
    prev: &PersistentState,
) -> Result<(FrameHeader, usize), HeaderError> {
    let mut r = BitReader::new(data);

    let frame_marker = r.f(2);
    if frame_marker != 2 {
        return Err(HeaderError::InvalidFrameMarker);
    }

    let profile_low_bit = r.f(1);
    let profile_high_bit = r.f(1);
    let profile = ((profile_high_bit << 1) + profile_low_bit) as u8;
    if profile == 3 {
        let _reserved_zero = r.f(1);
    }

    let show_existing_frame = r.flag();
    if show_existing_frame {
        let frame_to_show_map_idx = r.f(3) as u8;
        let consumed = r.byte_position_ceil();
        return Ok((
            FrameHeader::ShowExistingFrame {
                frame_to_show_map_idx,
            },
            consumed,
        ));
    }

    let frame_type = if r.f(1) == 0 {
        FrameType::KeyFrame
    } else {
        FrameType::NonKeyFrame
    };
    let show_frame = r.flag();
    let error_resilient_mode = r.flag();

    let mut ref_frame_idx = [0u8; 3];
    let mut ref_frame_sign_bias = [false; 4];
    let mut allow_high_precision_mv = false;
    let mut interpolation_filter = SWITCHABLE;
    let mut intra_only = false;
    let mut reset_frame_context = 0u8;

    let (color_config, width, height, refresh_frame_flags, frame_is_intra) = if frame_type
        == FrameType::KeyFrame
    {
        verify_sync_code(&mut r)?;
        let cc = Some(parse_color_config(&mut r, profile)?);
        let (w, h) = parse_frame_size(&mut r);
        // refresh_frame_flags is not read from the bitstream and is always 0xFF for key frames.
        (cc, w, h, 0xFFu8, true)
    } else {
        intra_only = !show_frame && r.flag();
        if !error_resilient_mode {
            reset_frame_context = r.f(2) as u8;
        }
        if intra_only {
            verify_sync_code(&mut r)?;
            let cc = Some(if profile > 0 {
                parse_color_config(&mut r, profile)?
            } else {
                ColorConfig::default_intra_only_profile0()
            });
            let rff = r.f(8) as u8;
            let (w, h) = parse_frame_size(&mut r);
            (cc, w, h, rff, true)
        } else {
            let rff = r.f(8) as u8;
            for i in 0..3 {
                ref_frame_idx[i] = r.f(3) as u8;
                ref_frame_sign_bias[LAST_FRAME as usize + i] = r.flag();
            }
            let (w, h) = parse_frame_size_with_refs(&mut r, ref_frame_idx, &prev.ref_frame_sizes)?;
            (None, w, h, rff, false)
        }
    };
    let (render_width, render_height) = parse_render_size(&mut r, width, height);
    if !frame_is_intra {
        allow_high_precision_mv = r.flag();
        interpolation_filter = parse_interpolation_filter(&mut r);
    }

    let (refresh_frame_context, frame_parallel_decoding_mode) = if !error_resilient_mode {
        (r.flag(), r.flag())
    } else {
        (false, true)
    };
    // The raw f(2) value is the single source of truth; the index used for load_probs/save_probs
    // (forced to 0 when FrameIsIntra || error_resilient_mode) is derived on read by
    // NewFrameHeader::effective_frame_context_idx().
    let frame_context_idx = r.f(2) as u8;

    // setup_past_independence(): when FrameIsIntra || error_resilient_mode, the
    // loop filter deltas and segmentation feature data are also reset to their
    // default values (same condition as the frame_context reset).
    let lf_reset = frame_is_intra || error_resilient_mode;
    let loop_filter = parse_loop_filter_params(&mut r, lf_reset, prev.loop_filter_deltas);
    let quantization = parse_quantization_params(&mut r);
    let segmentation = parse_segmentation_params(&mut r, lf_reset, prev.segmentation);

    let image_size = compute_image_size(width, height);
    let (tile_cols_log2, tile_rows_log2) = parse_tile_info(&mut r, image_size.sb64_cols);

    let header_size_in_bytes = r.f(16) as u16;

    let consumed = r.byte_position_ceil();

    Ok((
        FrameHeader::New(NewFrameHeader {
            profile,
            frame_type,
            show_frame,
            error_resilient_mode,
            frame_is_intra,
            intra_only,
            reset_frame_context,
            color_config,
            width,
            height,
            render_width,
            render_height,
            refresh_frame_flags,
            ref_frame_idx,
            ref_frame_sign_bias,
            allow_high_precision_mv,
            interpolation_filter,
            refresh_frame_context,
            frame_parallel_decoding_mode,
            frame_context_idx,
            loop_filter,
            quantization,
            segmentation,
            tile_cols_log2,
            tile_rows_log2,
            header_size_in_bytes,
        }),
        consumed,
    ))
}
