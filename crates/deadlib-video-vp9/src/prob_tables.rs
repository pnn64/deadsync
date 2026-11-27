//! Tree definitions and default probability tables used to decode VP9 syntax elements
//! (spec §9.3.1 "Tree selection process" and §§10.2-10.5).
//!
//! All values in this file are transcribed from the corresponding sections of the
//! VP9 Bitstream & Decoding Process Specification v0.7; no existing OSS implementation
//! was consulted (clean-room implementation). The source section number for each table
//! is noted in its doc comment.
//!
//! Covers both keyframe intra decoding (M2) and inter prediction (M3): partition/mode
//! trees and default probabilities, coefficient tables, and the MV-related tables.

// ---------------------------------------------------------------------------
// Syntax element values (spec §7.4.3, §7.3.1, and others).
// ---------------------------------------------------------------------------

/// `BLOCK_SIZES` (spec §3, value 13). Block size enum values
/// (confirmed by back-derivation from the fact that the PARTITION_NONE row of
/// `subsize_lookup` in spec §10.2 is exactly the BLOCK_SIZES ordering).
pub const BLOCK_4X4: u8 = 0;
pub const BLOCK_4X8: u8 = 1;
pub const BLOCK_8X4: u8 = 2;
pub const BLOCK_8X8: u8 = 3;
pub const BLOCK_8X16: u8 = 4;
pub const BLOCK_16X8: u8 = 5;
pub const BLOCK_16X16: u8 = 6;
pub const BLOCK_16X32: u8 = 7;
pub const BLOCK_32X16: u8 = 8;
pub const BLOCK_32X32: u8 = 9;
pub const BLOCK_32X64: u8 = 10;
pub const BLOCK_64X32: u8 = 11;
pub const BLOCK_64X64: u8 = 12;
/// Sentinel value representing an invalid partition/block-size combination (the spec
/// (§3) uses value 14, but this implementation never uses it as an array index, so
/// any distinguishable value works).
pub const BLOCK_INVALID: u8 = 0xFF;

/// `PARTITION_TYPES` (spec §7.4.3).
pub const PARTITION_NONE: u8 = 0;
pub const PARTITION_HORZ: u8 = 1;
pub const PARTITION_VERT: u8 = 2;
pub const PARTITION_SPLIT: u8 = 3;

/// `TX_SIZES` (spec §6.4.10 and others).
pub const TX_4X4: u8 = 0;
pub const TX_8X8: u8 = 1;
pub const TX_16X16: u8 = 2;
pub const TX_32X32: u8 = 3;

/// `TX_MODES` (spec §7.3.1).
pub const TX_MODE_SELECT: u8 = 4;

/// Intra prediction modes (`INTRA_MODES`, spec §7.4.5).
pub const DC_PRED: u8 = 0;
pub const V_PRED: u8 = 1;
pub const H_PRED: u8 = 2;
pub const D45_PRED: u8 = 3;
pub const D135_PRED: u8 = 4;
pub const D117_PRED: u8 = 5;
pub const D153_PRED: u8 = 6;
pub const D207_PRED: u8 = 7;
pub const D63_PRED: u8 = 8;
pub const TM_PRED: u8 = 9;

/// Inter prediction modes (the inter-side values `y_mode` can take, spec §7.4.11).
/// Defined as a continuation of the same `y_mode` namespace as the intra modes (0..9).
pub const NEARESTMV: u8 = 10;
pub const NEARMV: u8 = 11;
pub const ZEROMV: u8 = 12;
pub const NEWMV: u8 = 13;

/// `ref_frame[ 0 ]`/`ref_frame[ 1 ]` values (spec §7.4.12). `ref_frame[ 1 ] == 0` means
/// `NONE` (single prediction or intra), which coincides numerically with `INTRA_FRAME`
/// (the spec reuses the same 0 because the two meanings never apply at the same time).
/// `INTRA_FRAME` itself is defined in `common` (shared with `loop_filter.rs`, which used to
/// redefine it privately as a `usize`) and re-exported here so existing import paths
/// (`crate::prob_tables::INTRA_FRAME`, and `header.rs`'s further re-export of it) keep working.
pub use crate::common::INTRA_FRAME;
pub const LAST_FRAME: u8 = 1;
pub const GOLDEN_FRAME: u8 = 2;
pub const ALTREF_FRAME: u8 = 3;
/// Sentinel value for `ref_frame[ 1 ] == NONE` (single prediction). Same as `INTRA_FRAME`'s 0.
pub const REF_NONE: u8 = 0;

/// `interpolation_filter`/`interp_filter` values (spec §7.2.7).
pub const EIGHTTAP: u8 = 0;
pub const EIGHTTAP_SMOOTH: u8 = 1;
pub const EIGHTTAP_SHARP: u8 = 2;
pub const BILINEAR: u8 = 3;
pub const SWITCHABLE: u8 = 4;

/// `reference_mode` values (spec §7.3.6).
pub const SINGLE_REFERENCE: u8 = 0;
pub const COMPOUND_REFERENCE: u8 = 1;
pub const REFERENCE_MODE_SELECT: u8 = 2;

/// `mv_joint` values (spec §7.4.13).
pub const MV_JOINT_ZERO: u8 = 0;
pub const MV_JOINT_HNZVZ: u8 = 1;
pub const MV_JOINT_HZVNZ: u8 = 2;
pub const MV_JOINT_HNZVNZ: u8 = 3;

/// Number of reference frame slots (spec §7.2, index range for `RefFrameWidth`/`RefFrameHeight`, etc.).
pub const NUM_REF_FRAMES: usize = 8;

// ---------------------------------------------------------------------------
// Conversion lookup tables (spec §10.2 "Conversion tables").
// ---------------------------------------------------------------------------

/// `b_width_log2_lookup[ BLOCK_SIZES ]`。
pub const B_WIDTH_LOG2_LOOKUP: [u8; 13] = [0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4];
/// `b_height_log2_lookup[ BLOCK_SIZES ]`。
pub const B_HEIGHT_LOG2_LOOKUP: [u8; 13] = [0, 1, 0, 1, 2, 1, 2, 3, 2, 3, 4, 3, 4];
/// `num_4x4_blocks_wide_lookup[ BLOCK_SIZES ]`。
pub const NUM_4X4_BLOCKS_WIDE_LOOKUP: [u8; 13] = [1, 1, 2, 2, 2, 4, 4, 4, 8, 8, 8, 16, 16];
/// `num_4x4_blocks_high_lookup[ BLOCK_SIZES ]`。
pub const NUM_4X4_BLOCKS_HIGH_LOOKUP: [u8; 13] = [1, 2, 1, 2, 4, 2, 4, 8, 4, 8, 16, 8, 16];
/// `mi_width_log2_lookup[ BLOCK_SIZES ]`。
pub const MI_WIDTH_LOG2_LOOKUP: [u8; 13] = [0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3];
/// `num_8x8_blocks_wide_lookup[ BLOCK_SIZES ]`。
pub const NUM_8X8_BLOCKS_WIDE_LOOKUP: [u8; 13] = [1, 1, 1, 1, 1, 2, 2, 2, 4, 4, 4, 8, 8];
/// `mi_height_log2_lookup[ BLOCK_SIZES ]`。
/// `num_8x8_blocks_high_lookup[ BLOCK_SIZES ]`。
pub const NUM_8X8_BLOCKS_HIGH_LOOKUP: [u8; 13] = [1, 1, 1, 1, 2, 1, 2, 4, 2, 4, 8, 4, 8];

/// `max_txsize_lookup[ BLOCK_SIZES ]` (spec §6.4.10).
pub const MAX_TXSIZE_LOOKUP: [u8; 13] = [
    TX_4X4, TX_4X4, TX_4X4, TX_8X8, TX_8X8, TX_8X8, TX_16X16, TX_16X16, TX_16X16, TX_32X32,
    TX_32X32, TX_32X32, TX_32X32,
];

/// `tx_mode_to_biggest_tx_size[ TX_MODES ]` (spec §10.2).
pub const TX_MODE_TO_BIGGEST_TX_SIZE: [u8; 5] = [TX_4X4, TX_8X8, TX_16X16, TX_32X32, TX_32X32];

/// `subsize_lookup[ PARTITION_TYPES ][ BLOCK_SIZES ]` (spec §10.2).
/// `BLOCK_INVALID` is a sentinel value indicating that the partition/bsize combination
/// never occurs per the spec.
pub const SUBSIZE_LOOKUP: [[u8; 13]; 4] = [
    // PARTITION_NONE
    [
        BLOCK_4X4,
        BLOCK_4X8,
        BLOCK_8X4,
        BLOCK_8X8,
        BLOCK_8X16,
        BLOCK_16X8,
        BLOCK_16X16,
        BLOCK_16X32,
        BLOCK_32X16,
        BLOCK_32X32,
        BLOCK_32X64,
        BLOCK_64X32,
        BLOCK_64X64,
    ],
    // PARTITION_HORZ
    [
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_8X4,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_16X8,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_32X16,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_64X32,
    ],
    // PARTITION_VERT
    [
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_4X8,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_8X16,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_16X32,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_32X64,
    ],
    // PARTITION_SPLIT
    [
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_4X4,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_8X8,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_16X16,
        BLOCK_INVALID,
        BLOCK_INVALID,
        BLOCK_32X32,
    ],
];

// ---------------------------------------------------------------------------
// Tree definitions (spec §9.3.1 "Tree selection process").
//
// Leaves are `-value` (0 is treated as-is, since `-0 == 0`); internal nodes are
// non-negative values pointing to the next index. Used by
// [`crate::bool_coder::BoolDecoder::read_tree`].
// ---------------------------------------------------------------------------

/// `partition_tree[ 6 ]`. Used when hasRows == 1 && hasCols == 1.
pub const PARTITION_TREE: [i32; 6] = [
    -(PARTITION_NONE as i32),
    2,
    -(PARTITION_HORZ as i32),
    4,
    -(PARTITION_VERT as i32),
    -(PARTITION_SPLIT as i32),
];
/// `segment_tree[ 14 ]` (spec §9.3.1). Used to decode `segment_id`.
pub const SEGMENT_TREE: [i32; 14] = [2, 4, 6, 8, 10, 12, 0, -1, -2, -3, -4, -5, -6, -7];

/// `intra_mode_tree[ 18 ]`. Used to decode `default_intra_mode`/`default_uv_mode`/
/// `intra_mode`/`sub_intra_mode`/`uv_mode`.
pub const INTRA_MODE_TREE: [i32; 18] = [
    -(DC_PRED as i32),
    2,
    -(TM_PRED as i32),
    4,
    -(V_PRED as i32),
    6,
    8,
    12,
    -(H_PRED as i32),
    10,
    -(D135_PRED as i32),
    -(D117_PRED as i32),
    -(D45_PRED as i32),
    14,
    -(D63_PRED as i32),
    16,
    -(D153_PRED as i32),
    -(D207_PRED as i32),
];

/// `tx_size_32_tree[ 6 ]`. Used when maxTxSize == TX_32X32.
pub const TX_SIZE_32_TREE: [i32; 6] = [
    -(TX_4X4 as i32),
    2,
    -(TX_8X8 as i32),
    4,
    -(TX_16X16 as i32),
    -(TX_32X32 as i32),
];
/// `tx_size_16_tree[ 4 ]`. Used when maxTxSize == TX_16X16.
pub const TX_SIZE_16_TREE: [i32; 4] = [-(TX_4X4 as i32), 2, -(TX_8X8 as i32), -(TX_16X16 as i32)];
/// `tx_size_8_tree[ 2 ]`. Used when maxTxSize == TX_8X8 (otherwise tx_size is not read).
pub const TX_SIZE_8_TREE: [i32; 2] = [-(TX_4X4 as i32), -(TX_8X8 as i32)];

/// `inter_mode_tree[ 6 ]` (spec §9.3.1). Leaf values are `y_mode - NEARESTMV`.
pub const INTER_MODE_TREE: [i32; 6] = [
    -((ZEROMV - NEARESTMV) as i32),
    2,
    0, // -(NEARESTMV - NEARESTMV) is always 0 (NEARESTMV is the base value).
    4,
    -((NEARMV - NEARESTMV) as i32),
    -((NEWMV - NEARESTMV) as i32),
];

/// `interp_filter_tree[ 4 ]` (spec §9.3.1). `BILINEAR` can only be selected at the frame
/// level and never appears in the block-level `interp_filter` tree.
pub const INTERP_FILTER_TREE: [i32; 4] = [
    -(EIGHTTAP as i32),
    2,
    -(EIGHTTAP_SMOOTH as i32),
    -(EIGHTTAP_SHARP as i32),
];

/// `mv_joint_tree[ 6 ]` (spec §9.3.1).
pub const MV_JOINT_TREE: [i32; 6] = [
    -(MV_JOINT_ZERO as i32),
    2,
    -(MV_JOINT_HNZVZ as i32),
    4,
    -(MV_JOINT_HZVNZ as i32),
    -(MV_JOINT_HNZVNZ as i32),
];

/// `mv_class_tree[ 20 ]` (spec §9.3.1). 11 values, `MV_CLASS_0`..`MV_CLASS_10`.
pub const MV_CLASS_TREE: [i32; 20] = [
    0, 2, -1, 4, 6, 8, -2, -3, 10, 12, -4, -5, -6, 14, 16, 18, -7, -8, -9, -10,
];

/// `mv_fr_tree[ 6 ]` (spec §9.3.1). Used to decode `mv_class0_fr`/`mv_fr` (values 0..3).
pub const MV_FR_TREE: [i32; 6] = [0, 2, -1, 4, -2, -3];

// ---------------------------------------------------------------------------
// Fixed probability tables (spec §10.4 "Fixed probability tables").
// Used only for partition/intra mode decoding in keyframes and intra-only frames;
// never updated by compressed_header.
// ---------------------------------------------------------------------------

/// Fixed partition probability table for keyframes (spec §10.4 `kf_partition_probs`).
pub const KF_PARTITION_PROBS: [[u8; 3]; 16] = [
    [158, 97, 94], // 8x8 -> 4x4: a/l both not split
    [93, 24, 99],  // 8x8 -> 4x4: a split, l not split
    [85, 119, 44], // 8x8 -> 4x4: l split, a not split
    [62, 59, 67],  // 8x8 -> 4x4: a/l both split
    [149, 53, 53], // 16x16 -> 8x8: a/l both not split
    [94, 20, 48],  // 16x16 -> 8x8: a split, l not split
    [83, 53, 24],  // 16x16 -> 8x8: l split, a not split
    [52, 18, 18],  // 16x16 -> 8x8: a/l both split
    [150, 40, 39], // 32x32 -> 16x16: a/l both not split
    [78, 12, 26],  // 32x32 -> 16x16: a split, l not split
    [67, 33, 11],  // 32x32 -> 16x16: l split, a not split
    [24, 7, 5],    // 32x32 -> 16x16: a/l both split
    [174, 35, 49], // 64x64 -> 32x32: a/l both not split
    [68, 11, 27],  // 64x64 -> 32x32: a split, l not split
    [57, 15, 9],   // 64x64 -> 32x32: l split, a not split
    [12, 3, 3],    // 64x64 -> 32x32: a/l both split
];

/// Fixed y_mode (default_intra_mode) probability table for keyframes
/// (spec §10.4 `kf_y_mode_probs[above][left][node]`).
pub const KF_Y_MODE_PROBS: [[[u8; 9]; 10]; 10] = [
    [
        // above = DC
        [137, 30, 42, 148, 151, 207, 70, 52, 91], // left = DC
        [92, 45, 102, 136, 116, 180, 74, 90, 100], // left = V
        [73, 32, 19, 187, 222, 215, 46, 34, 100], // left = H
        [91, 30, 32, 116, 121, 186, 93, 86, 94],  // left = D45
        [72, 35, 36, 149, 68, 206, 68, 63, 105],  // left = D135
        [73, 31, 28, 138, 57, 124, 55, 122, 151], // left = D117
        [67, 23, 21, 140, 126, 197, 40, 37, 171], // left = D153
        [86, 27, 28, 128, 154, 212, 45, 43, 53],  // left = D207
        [74, 32, 27, 107, 86, 160, 63, 134, 102], // left = D63
        [59, 67, 44, 140, 161, 202, 78, 67, 119], // left = TM
    ],
    [
        // above = V
        [63, 36, 126, 146, 123, 158, 60, 90, 96], // left = DC
        [43, 46, 168, 134, 107, 128, 69, 142, 92], // left = V
        [44, 29, 68, 159, 201, 177, 50, 57, 77],  // left = H
        [58, 38, 76, 114, 97, 172, 78, 133, 92],  // left = D45
        [46, 41, 76, 140, 63, 184, 69, 112, 57],  // left = D135
        [38, 32, 85, 140, 46, 112, 54, 151, 133], // left = D117
        [39, 27, 61, 131, 110, 175, 44, 75, 136], // left = D153
        [52, 30, 74, 113, 130, 175, 51, 64, 58],  // left = D207
        [47, 35, 80, 100, 74, 143, 64, 163, 74],  // left = D63
        [36, 61, 116, 114, 128, 162, 80, 125, 82], // left = TM
    ],
    [
        // above = H
        [82, 26, 26, 171, 208, 204, 44, 32, 105], // left = DC
        [55, 44, 68, 166, 179, 192, 57, 57, 108], // left = V
        [42, 26, 11, 199, 241, 228, 23, 15, 85],  // left = H
        [68, 42, 19, 131, 160, 199, 55, 52, 83],  // left = D45
        [58, 50, 25, 139, 115, 232, 39, 52, 118], // left = D135
        [50, 35, 33, 153, 104, 162, 64, 59, 131], // left = D117
        [44, 24, 16, 150, 177, 202, 33, 19, 156], // left = D153
        [55, 27, 12, 153, 203, 218, 26, 27, 49],  // left = D207
        [53, 49, 21, 110, 116, 168, 59, 80, 76],  // left = D63
        [38, 72, 19, 168, 203, 212, 50, 50, 107], // left = TM
    ],
    [
        // above = D45
        [103, 26, 36, 129, 132, 201, 83, 80, 93], // left = DC
        [59, 38, 83, 112, 103, 162, 98, 136, 90], // left = V
        [62, 30, 23, 158, 200, 207, 59, 57, 50],  // left = H
        [67, 30, 29, 84, 86, 191, 102, 91, 59],   // left = D45
        [60, 32, 33, 112, 71, 220, 64, 89, 104],  // left = D135
        [53, 26, 34, 130, 56, 149, 84, 120, 103], // left = D117
        [53, 21, 23, 133, 109, 210, 56, 77, 172], // left = D153
        [77, 19, 29, 112, 142, 228, 55, 66, 36],  // left = D207
        [61, 29, 29, 93, 97, 165, 83, 175, 162],  // left = D63
        [47, 47, 43, 114, 137, 181, 100, 99, 95], // left = TM
    ],
    [
        // above = D135
        [69, 23, 29, 128, 83, 199, 46, 44, 101],  // left = DC
        [53, 40, 55, 139, 69, 183, 61, 80, 110],  // left = V
        [40, 29, 19, 161, 180, 207, 43, 24, 91],  // left = H
        [60, 34, 19, 105, 61, 198, 53, 64, 89],   // left = D45
        [52, 31, 22, 158, 40, 209, 58, 62, 89],   // left = D135
        [44, 31, 29, 147, 46, 158, 56, 102, 198], // left = D117
        [35, 19, 12, 135, 87, 209, 41, 45, 167],  // left = D153
        [55, 25, 21, 118, 95, 215, 38, 39, 66],   // left = D207
        [51, 38, 25, 113, 58, 164, 70, 93, 97],   // left = D63
        [47, 54, 34, 146, 108, 203, 72, 103, 151], // left = TM
    ],
    [
        // above = D117
        [64, 19, 37, 156, 66, 138, 49, 95, 133],  // left = DC
        [46, 27, 80, 150, 55, 124, 55, 121, 135], // left = V
        [36, 23, 27, 165, 149, 166, 54, 64, 118], // left = H
        [53, 21, 36, 131, 63, 163, 60, 109, 81],  // left = D45
        [40, 26, 35, 154, 40, 185, 51, 97, 123],  // left = D135
        [35, 19, 34, 179, 19, 97, 48, 129, 124],  // left = D117
        [36, 20, 26, 136, 62, 164, 33, 77, 154],  // left = D153
        [45, 18, 32, 130, 90, 157, 40, 79, 91],   // left = D207
        [45, 26, 28, 129, 45, 129, 49, 147, 123], // left = D63
        [38, 44, 51, 136, 74, 162, 57, 97, 121],  // left = TM
    ],
    [
        // above = D153
        [75, 17, 22, 136, 138, 185, 32, 34, 166], // left = DC
        [56, 39, 58, 133, 117, 173, 48, 53, 187], // left = V
        [35, 21, 12, 161, 212, 207, 20, 23, 145], // left = H
        [56, 29, 19, 117, 109, 181, 55, 68, 112], // left = D45
        [47, 29, 17, 153, 64, 220, 59, 51, 114],  // left = D135
        [46, 16, 24, 136, 76, 147, 41, 64, 172],  // left = D117
        [34, 17, 11, 108, 152, 187, 13, 15, 209], // left = D153
        [51, 24, 14, 115, 133, 209, 32, 26, 104], // left = D207
        [55, 30, 18, 122, 79, 179, 44, 88, 116],  // left = D63
        [37, 49, 25, 129, 168, 164, 41, 54, 148], // left = TM
    ],
    [
        // above = D207
        [82, 22, 32, 127, 143, 213, 39, 41, 70],  // left = DC
        [62, 44, 61, 123, 105, 189, 48, 57, 64],  // left = V
        [47, 25, 17, 175, 222, 220, 24, 30, 86],  // left = H
        [68, 36, 17, 106, 102, 206, 59, 74, 74],  // left = D45
        [57, 39, 23, 151, 68, 216, 55, 63, 58],   // left = D135
        [49, 30, 35, 141, 70, 168, 82, 40, 115],  // left = D117
        [51, 25, 15, 136, 129, 202, 38, 35, 139], // left = D153
        [68, 26, 16, 111, 141, 215, 29, 28, 28],  // left = D207
        [59, 39, 19, 114, 75, 180, 77, 104, 42],  // left = D63
        [40, 61, 26, 126, 152, 206, 61, 59, 93],  // left = TM
    ],
    [
        // above = D63
        [78, 23, 39, 111, 117, 170, 74, 124, 94], // left = DC
        [48, 34, 86, 101, 92, 146, 78, 179, 134], // left = V
        [47, 22, 24, 138, 187, 178, 68, 69, 59],  // left = H
        [56, 25, 33, 105, 112, 187, 95, 177, 129], // left = D45
        [48, 31, 27, 114, 63, 183, 82, 116, 56],  // left = D135
        [43, 28, 37, 121, 63, 123, 61, 192, 169], // left = D117
        [42, 17, 24, 109, 97, 177, 56, 76, 122],  // left = D153
        [58, 18, 28, 105, 139, 182, 70, 92, 63],  // left = D207
        [46, 23, 32, 74, 86, 150, 67, 183, 88],   // left = D63
        [36, 38, 48, 92, 122, 165, 88, 137, 91],  // left = TM
    ],
    [
        // above = TM
        [65, 70, 60, 155, 159, 199, 61, 60, 81], // left = DC
        [44, 78, 115, 132, 119, 173, 71, 112, 93], // left = V
        [39, 38, 21, 184, 227, 206, 42, 32, 64], // left = H
        [58, 47, 36, 124, 137, 193, 80, 82, 78], // left = D45
        [49, 50, 35, 144, 95, 205, 63, 78, 59],  // left = D135
        [41, 53, 52, 148, 71, 142, 65, 128, 51], // left = D117
        [40, 36, 28, 143, 143, 202, 40, 55, 137], // left = D153
        [52, 34, 29, 129, 183, 227, 42, 35, 43], // left = D207
        [42, 44, 44, 104, 105, 164, 64, 130, 80], // left = D63
        [43, 81, 53, 140, 169, 204, 68, 84, 72], // left = TM
    ],
];

/// Fixed uv_mode probability table for keyframes (spec §10.4 `kf_uv_mode_probs[y_mode][node]`).
pub const KF_UV_MODE_PROBS: [[u8; 9]; 10] = [
    [144, 11, 54, 157, 195, 130, 46, 58, 108],  // y = DC
    [118, 15, 123, 148, 131, 101, 44, 93, 131], // y = V
    [113, 12, 23, 188, 226, 142, 26, 32, 125],  // y = H
    [120, 11, 50, 123, 163, 135, 64, 77, 103],  // y = D45
    [113, 9, 36, 155, 111, 157, 32, 44, 161],   // y = D135
    [116, 9, 55, 176, 76, 96, 37, 61, 149],     // y = D117
    [115, 9, 28, 141, 161, 167, 21, 25, 193],   // y = D153
    [120, 12, 32, 145, 195, 142, 32, 38, 86],   // y = D207
    [116, 12, 64, 120, 140, 125, 49, 115, 121], // y = D63
    [102, 19, 66, 162, 182, 122, 35, 59, 128],  // y = TM
];

// ---------------------------------------------------------------------------
// Table used by inv_remap_prob (spec §6.3.5).
// ---------------------------------------------------------------------------

/// `inv_map_table` (spec §6.3.5). Used by `inv_remap_prob`.
pub const INV_MAP_TABLE: [u8; 255] = [
    7, 20, 33, 46, 59, 72, 85, 98, 111, 124, 137, 150, 163, 176, 189, 202, 215, 228, 241, 254, 1,
    2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28,
    29, 30, 31, 32, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 47, 48, 49, 50, 51, 52, 53, 54,
    55, 56, 57, 58, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 73, 74, 75, 76, 77, 78, 79, 80,
    81, 82, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 99, 100, 101, 102, 103, 104,
    105, 106, 107, 108, 109, 110, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 125,
    126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 138, 139, 140, 141, 142, 143, 144, 145,
    146, 147, 148, 149, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 164, 165, 166,
    167, 168, 169, 170, 171, 172, 173, 174, 175, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186,
    187, 188, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 203, 204, 205, 206, 207,
    208, 209, 210, 211, 212, 213, 214, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227,
    229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 242, 243, 244, 245, 246, 247, 248,
    249, 250, 251, 252, 253, 253,
];

// ---------------------------------------------------------------------------
// Default probability tables (spec §10.5 "Default probability tables").
// Initial values before being updated by compressed_header's diff_update_prob.
// ---------------------------------------------------------------------------

/// `default_tx_probs` (spec §10.5). Initial values for `tx_probs[maxTxSize][ctx][node]`.
/// The first entry (TX_4X4) is unused per the spec, but kept to transcribe the
/// structure as-is.
pub const DEFAULT_TX_PROBS: [[[u8; 3]; 2]; 4] = [
    [
        // maxTxSize = TX_4X4 (unused)
        [0, 0, 0],
        [0, 0, 0],
    ],
    [
        // maxTxSize = TX_8X8
        [100, 0, 0],
        [66, 0, 0],
    ],
    [
        // maxTxSize = TX_16X16
        [20, 152, 0],
        [15, 101, 0],
    ],
    [
        // maxTxSize = TX_32X32
        [3, 136, 37],
        [5, 52, 13],
    ],
];

/// `default_skip_prob` (spec §10.5).
pub const DEFAULT_SKIP_PROB: [u8; 3] = [192, 128, 64];

/// Type of `coef_probs[ txSz ][ plane > 0 ][ is_inter ][ band ][ ctx ][ node ]`
/// (spec §6.3.7 `read_coef_probs`).
pub type CoefProbs = [[[[[[u8; 3]; 6]; 6]; 2]; 2]; 4];

/// `default_coef_probs` (spec §10.5). Initial values for `[txSz][plane>0][is_inter][band][ctx][node]`.
/// Band 0 has only 3 contexts, so the remaining 3 elements are zero-filled as "unused"
/// per the spec (the source also explicitly states `{0, 0, 0}, // unused`).
pub const DEFAULT_COEF_PROBS: CoefProbs = [
    // TX_4X4
    [
        // block type: Y
        [
            // Intra
            [
                // band 0
                [
                    [195, 29, 183],
                    [84, 49, 136],
                    [8, 42, 71],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [31, 107, 169],
                    [35, 99, 159],
                    [17, 82, 140],
                    [8, 66, 114],
                    [2, 44, 76],
                    [1, 19, 32],
                ],
                // band 2
                [
                    [40, 132, 201],
                    [29, 114, 187],
                    [13, 91, 157],
                    [7, 75, 127],
                    [3, 58, 95],
                    [1, 28, 47],
                ],
                // band 3
                [
                    [69, 142, 221],
                    [42, 122, 201],
                    [15, 91, 159],
                    [6, 67, 121],
                    [1, 42, 77],
                    [1, 17, 31],
                ],
                // band 4
                [
                    [102, 148, 228],
                    [67, 117, 204],
                    [17, 82, 154],
                    [6, 59, 114],
                    [2, 39, 75],
                    [1, 15, 29],
                ],
                // band 5
                [
                    [156, 57, 233],
                    [119, 57, 212],
                    [58, 48, 163],
                    [29, 40, 124],
                    [12, 30, 81],
                    [3, 12, 31],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [191, 107, 226],
                    [124, 117, 204],
                    [25, 99, 155],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [29, 148, 210],
                    [37, 126, 194],
                    [8, 93, 157],
                    [2, 68, 118],
                    [1, 39, 69],
                    [1, 17, 33],
                ],
                // band 2
                [
                    [41, 151, 213],
                    [27, 123, 193],
                    [3, 82, 144],
                    [1, 58, 105],
                    [1, 32, 60],
                    [1, 13, 26],
                ],
                // band 3
                [
                    [59, 159, 220],
                    [23, 126, 198],
                    [4, 88, 151],
                    [1, 66, 114],
                    [1, 38, 71],
                    [1, 18, 34],
                ],
                // band 4
                [
                    [114, 136, 232],
                    [51, 114, 207],
                    [11, 83, 155],
                    [3, 56, 105],
                    [1, 33, 65],
                    [1, 17, 34],
                ],
                // band 5
                [
                    [149, 65, 234],
                    [121, 57, 215],
                    [61, 49, 166],
                    [28, 36, 114],
                    [12, 25, 76],
                    [3, 16, 42],
                ],
            ],
        ],
        // block type: UV
        [
            // Intra
            [
                // band 0
                [
                    [214, 49, 220],
                    [132, 63, 188],
                    [42, 65, 137],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [85, 137, 221],
                    [104, 131, 216],
                    [49, 111, 192],
                    [21, 87, 155],
                    [2, 49, 87],
                    [1, 16, 28],
                ],
                // band 2
                [
                    [89, 163, 230],
                    [90, 137, 220],
                    [29, 100, 183],
                    [10, 70, 135],
                    [2, 42, 81],
                    [1, 17, 33],
                ],
                // band 3
                [
                    [108, 167, 237],
                    [55, 133, 222],
                    [15, 97, 179],
                    [4, 72, 135],
                    [1, 45, 85],
                    [1, 19, 38],
                ],
                // band 4
                [
                    [124, 146, 240],
                    [66, 124, 224],
                    [17, 88, 175],
                    [4, 58, 122],
                    [1, 36, 75],
                    [1, 18, 37],
                ],
                // band 5
                [
                    [141, 79, 241],
                    [126, 70, 227],
                    [66, 58, 182],
                    [30, 44, 136],
                    [12, 34, 96],
                    [2, 20, 47],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [229, 99, 249],
                    [143, 111, 235],
                    [46, 109, 192],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [82, 158, 236],
                    [94, 146, 224],
                    [25, 117, 191],
                    [9, 87, 149],
                    [3, 56, 99],
                    [1, 33, 57],
                ],
                // band 2
                [
                    [83, 167, 237],
                    [68, 145, 222],
                    [10, 103, 177],
                    [2, 72, 131],
                    [1, 41, 79],
                    [1, 20, 39],
                ],
                // band 3
                [
                    [99, 167, 239],
                    [47, 141, 224],
                    [10, 104, 178],
                    [2, 73, 133],
                    [1, 44, 85],
                    [1, 22, 47],
                ],
                // band 4
                [
                    [127, 145, 243],
                    [71, 129, 228],
                    [17, 93, 177],
                    [3, 61, 124],
                    [1, 41, 84],
                    [1, 21, 52],
                ],
                // band 5
                [
                    [157, 78, 244],
                    [140, 72, 231],
                    [69, 58, 184],
                    [31, 44, 137],
                    [14, 38, 105],
                    [8, 23, 61],
                ],
            ],
        ],
    ],
    // TX_8X8
    [
        // block type: Y
        [
            // Intra
            [
                // band 0
                [
                    [125, 34, 187],
                    [52, 41, 133],
                    [6, 31, 56],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [37, 109, 153],
                    [51, 102, 147],
                    [23, 87, 128],
                    [8, 67, 101],
                    [1, 41, 63],
                    [1, 19, 29],
                ],
                // band 2
                [
                    [31, 154, 185],
                    [17, 127, 175],
                    [6, 96, 145],
                    [2, 73, 114],
                    [1, 51, 82],
                    [1, 28, 45],
                ],
                // band 3
                [
                    [23, 163, 200],
                    [10, 131, 185],
                    [2, 93, 148],
                    [1, 67, 111],
                    [1, 41, 69],
                    [1, 14, 24],
                ],
                // band 4
                [
                    [29, 176, 217],
                    [12, 145, 201],
                    [3, 101, 156],
                    [1, 69, 111],
                    [1, 39, 63],
                    [1, 14, 23],
                ],
                // band 5
                [
                    [57, 192, 233],
                    [25, 154, 215],
                    [6, 109, 167],
                    [3, 78, 118],
                    [1, 48, 69],
                    [1, 21, 29],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [202, 105, 245],
                    [108, 106, 216],
                    [18, 90, 144],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [33, 172, 219],
                    [64, 149, 206],
                    [14, 117, 177],
                    [5, 90, 141],
                    [2, 61, 95],
                    [1, 37, 57],
                ],
                // band 2
                [
                    [33, 179, 220],
                    [11, 140, 198],
                    [1, 89, 148],
                    [1, 60, 104],
                    [1, 33, 57],
                    [1, 12, 21],
                ],
                // band 3
                [
                    [30, 181, 221],
                    [8, 141, 198],
                    [1, 87, 145],
                    [1, 58, 100],
                    [1, 31, 55],
                    [1, 12, 20],
                ],
                // band 4
                [
                    [32, 186, 224],
                    [7, 142, 198],
                    [1, 86, 143],
                    [1, 58, 100],
                    [1, 31, 55],
                    [1, 12, 22],
                ],
                // band 5
                [
                    [57, 192, 227],
                    [20, 143, 204],
                    [3, 96, 154],
                    [1, 68, 112],
                    [1, 42, 69],
                    [1, 19, 32],
                ],
            ],
        ],
        // block type: UV
        [
            // Intra
            [
                // band 0
                [
                    [212, 35, 215],
                    [113, 47, 169],
                    [29, 48, 105],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [74, 129, 203],
                    [106, 120, 203],
                    [49, 107, 178],
                    [19, 84, 144],
                    [4, 50, 84],
                    [1, 15, 25],
                ],
                // band 2
                [
                    [71, 172, 217],
                    [44, 141, 209],
                    [15, 102, 173],
                    [6, 76, 133],
                    [2, 51, 89],
                    [1, 24, 42],
                ],
                // band 3
                [
                    [64, 185, 231],
                    [31, 148, 216],
                    [8, 103, 175],
                    [3, 74, 131],
                    [1, 46, 81],
                    [1, 18, 30],
                ],
                // band 4
                [
                    [65, 196, 235],
                    [25, 157, 221],
                    [5, 105, 174],
                    [1, 67, 120],
                    [1, 38, 69],
                    [1, 15, 30],
                ],
                // band 5
                [
                    [65, 204, 238],
                    [30, 156, 224],
                    [7, 107, 177],
                    [2, 70, 124],
                    [1, 42, 73],
                    [1, 18, 34],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [225, 86, 251],
                    [144, 104, 235],
                    [42, 99, 181],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [85, 175, 239],
                    [112, 165, 229],
                    [29, 136, 200],
                    [12, 103, 162],
                    [6, 77, 123],
                    [2, 53, 84],
                ],
                // band 2
                [
                    [75, 183, 239],
                    [30, 155, 221],
                    [3, 106, 171],
                    [1, 74, 128],
                    [1, 44, 76],
                    [1, 17, 28],
                ],
                // band 3
                [
                    [73, 185, 240],
                    [27, 159, 222],
                    [2, 107, 172],
                    [1, 75, 127],
                    [1, 42, 73],
                    [1, 17, 29],
                ],
                // band 4
                [
                    [62, 190, 238],
                    [21, 159, 222],
                    [2, 107, 172],
                    [1, 72, 122],
                    [1, 40, 71],
                    [1, 18, 32],
                ],
                // band 5
                [
                    [61, 199, 240],
                    [27, 161, 226],
                    [4, 113, 180],
                    [1, 76, 129],
                    [1, 46, 80],
                    [1, 23, 41],
                ],
            ],
        ],
    ],
    // TX_16X16
    [
        // block type: Y
        [
            // Intra
            [
                // band 0
                [
                    [7, 27, 153],
                    [5, 30, 95],
                    [1, 16, 30],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [50, 75, 127],
                    [57, 75, 124],
                    [27, 67, 108],
                    [10, 54, 86],
                    [1, 33, 52],
                    [1, 12, 18],
                ],
                // band 2
                [
                    [43, 125, 151],
                    [26, 108, 148],
                    [7, 83, 122],
                    [2, 59, 89],
                    [1, 38, 60],
                    [1, 17, 27],
                ],
                // band 3
                [
                    [23, 144, 163],
                    [13, 112, 154],
                    [2, 75, 117],
                    [1, 50, 81],
                    [1, 31, 51],
                    [1, 14, 23],
                ],
                // band 4
                [
                    [18, 162, 185],
                    [6, 123, 171],
                    [1, 78, 125],
                    [1, 51, 86],
                    [1, 31, 54],
                    [1, 14, 23],
                ],
                // band 5
                [
                    [15, 199, 227],
                    [3, 150, 204],
                    [1, 91, 146],
                    [1, 55, 95],
                    [1, 30, 53],
                    [1, 11, 20],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [19, 55, 240],
                    [19, 59, 196],
                    [3, 52, 105],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [41, 166, 207],
                    [104, 153, 199],
                    [31, 123, 181],
                    [14, 101, 152],
                    [5, 72, 106],
                    [1, 36, 52],
                ],
                // band 2
                [
                    [35, 176, 211],
                    [12, 131, 190],
                    [2, 88, 144],
                    [1, 60, 101],
                    [1, 36, 60],
                    [1, 16, 28],
                ],
                // band 3
                [
                    [28, 183, 213],
                    [8, 134, 191],
                    [1, 86, 142],
                    [1, 56, 96],
                    [1, 30, 53],
                    [1, 12, 20],
                ],
                // band 4
                [
                    [20, 190, 215],
                    [4, 135, 192],
                    [1, 84, 139],
                    [1, 53, 91],
                    [1, 28, 49],
                    [1, 11, 20],
                ],
                // band 5
                [
                    [13, 196, 216],
                    [2, 137, 192],
                    [1, 86, 143],
                    [1, 57, 99],
                    [1, 32, 56],
                    [1, 13, 24],
                ],
            ],
        ],
        // block type: UV
        [
            // Intra
            [
                // band 0
                [
                    [211, 29, 217],
                    [96, 47, 156],
                    [22, 43, 87],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [78, 120, 193],
                    [111, 116, 186],
                    [46, 102, 164],
                    [15, 80, 128],
                    [2, 49, 76],
                    [1, 18, 28],
                ],
                // band 2
                [
                    [71, 161, 203],
                    [42, 132, 192],
                    [10, 98, 150],
                    [3, 69, 109],
                    [1, 44, 70],
                    [1, 18, 29],
                ],
                // band 3
                [
                    [57, 186, 211],
                    [30, 140, 196],
                    [4, 93, 146],
                    [1, 62, 102],
                    [1, 38, 65],
                    [1, 16, 27],
                ],
                // band 4
                [
                    [47, 199, 217],
                    [14, 145, 196],
                    [1, 88, 142],
                    [1, 57, 98],
                    [1, 36, 62],
                    [1, 15, 26],
                ],
                // band 5
                [
                    [26, 219, 229],
                    [5, 155, 207],
                    [1, 94, 151],
                    [1, 60, 104],
                    [1, 36, 62],
                    [1, 16, 28],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [233, 29, 248],
                    [146, 47, 220],
                    [43, 52, 140],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [100, 163, 232],
                    [179, 161, 222],
                    [63, 142, 204],
                    [37, 113, 174],
                    [26, 89, 137],
                    [18, 68, 97],
                ],
                // band 2
                [
                    [85, 181, 230],
                    [32, 146, 209],
                    [7, 100, 164],
                    [3, 71, 121],
                    [1, 45, 77],
                    [1, 18, 30],
                ],
                // band 3
                [
                    [65, 187, 230],
                    [20, 148, 207],
                    [2, 97, 159],
                    [1, 68, 116],
                    [1, 40, 70],
                    [1, 14, 29],
                ],
                // band 4
                [
                    [40, 194, 227],
                    [8, 147, 204],
                    [1, 94, 155],
                    [1, 65, 112],
                    [1, 39, 66],
                    [1, 14, 26],
                ],
                // band 5
                [
                    [16, 208, 228],
                    [3, 151, 207],
                    [1, 98, 160],
                    [1, 67, 117],
                    [1, 41, 74],
                    [1, 17, 31],
                ],
            ],
        ],
    ],
    // TX_32X32
    [
        // block type: Y
        [
            // Intra
            [
                // band 0
                [
                    [17, 38, 140],
                    [7, 34, 80],
                    [1, 17, 29],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [37, 75, 128],
                    [41, 76, 128],
                    [26, 66, 116],
                    [12, 52, 94],
                    [2, 32, 55],
                    [1, 10, 16],
                ],
                // band 2
                [
                    [50, 127, 154],
                    [37, 109, 152],
                    [16, 82, 121],
                    [5, 59, 85],
                    [1, 35, 54],
                    [1, 13, 20],
                ],
                // band 3
                [
                    [40, 142, 167],
                    [17, 110, 157],
                    [2, 71, 112],
                    [1, 44, 72],
                    [1, 27, 45],
                    [1, 11, 17],
                ],
                // band 4
                [
                    [30, 175, 188],
                    [9, 124, 169],
                    [1, 74, 116],
                    [1, 48, 78],
                    [1, 30, 49],
                    [1, 11, 18],
                ],
                // band 5
                [
                    [10, 222, 223],
                    [2, 150, 194],
                    [1, 83, 128],
                    [1, 48, 79],
                    [1, 27, 45],
                    [1, 11, 17],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [36, 41, 235],
                    [29, 36, 193],
                    [10, 27, 111],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [85, 165, 222],
                    [177, 162, 215],
                    [110, 135, 195],
                    [57, 113, 168],
                    [23, 83, 120],
                    [10, 49, 61],
                ],
                // band 2
                [
                    [85, 190, 223],
                    [36, 139, 200],
                    [5, 90, 146],
                    [1, 60, 103],
                    [1, 38, 65],
                    [1, 18, 30],
                ],
                // band 3
                [
                    [72, 202, 223],
                    [23, 141, 199],
                    [2, 86, 140],
                    [1, 56, 97],
                    [1, 36, 61],
                    [1, 16, 27],
                ],
                // band 4
                [
                    [55, 218, 225],
                    [13, 145, 200],
                    [1, 86, 141],
                    [1, 57, 99],
                    [1, 35, 61],
                    [1, 13, 22],
                ],
                // band 5
                [
                    [15, 235, 212],
                    [1, 132, 184],
                    [1, 84, 139],
                    [1, 57, 97],
                    [1, 34, 56],
                    [1, 14, 23],
                ],
            ],
        ],
        // block type: UV
        [
            // Intra
            [
                // band 0
                [
                    [181, 21, 201],
                    [61, 37, 123],
                    [10, 38, 71],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [47, 106, 172],
                    [95, 104, 173],
                    [42, 93, 159],
                    [18, 77, 131],
                    [4, 50, 81],
                    [1, 17, 23],
                ],
                // band 2
                [
                    [62, 147, 199],
                    [44, 130, 189],
                    [28, 102, 154],
                    [18, 75, 115],
                    [2, 44, 65],
                    [1, 12, 19],
                ],
                // band 3
                [
                    [55, 153, 210],
                    [24, 130, 194],
                    [3, 93, 146],
                    [1, 61, 97],
                    [1, 31, 50],
                    [1, 10, 16],
                ],
                // band 4
                [
                    [49, 186, 223],
                    [17, 148, 204],
                    [1, 96, 142],
                    [1, 53, 83],
                    [1, 26, 44],
                    [1, 11, 17],
                ],
                // band 5
                [
                    [13, 217, 212],
                    [2, 136, 180],
                    [1, 78, 124],
                    [1, 50, 83],
                    [1, 29, 49],
                    [1, 14, 23],
                ],
            ],
            // Inter
            [
                // band 0
                [
                    [197, 13, 247],
                    [82, 17, 222],
                    [25, 17, 162],
                    [0, 0, 0],
                    [0, 0, 0],
                    [0, 0, 0],
                ],
                // band 1
                [
                    [126, 186, 247],
                    [234, 191, 243],
                    [176, 177, 234],
                    [104, 158, 220],
                    [66, 128, 186],
                    [55, 90, 137],
                ],
                // band 2
                [
                    [111, 197, 242],
                    [46, 158, 219],
                    [9, 104, 171],
                    [2, 65, 125],
                    [1, 44, 80],
                    [1, 17, 91],
                ],
                // band 3
                [
                    [104, 208, 245],
                    [39, 168, 224],
                    [3, 109, 162],
                    [1, 79, 124],
                    [1, 50, 102],
                    [1, 43, 102],
                ],
                // band 4
                [
                    [84, 220, 246],
                    [31, 177, 231],
                    [2, 115, 180],
                    [1, 79, 134],
                    [1, 55, 77],
                    [1, 60, 79],
                ],
                // band 5
                [
                    [43, 243, 240],
                    [8, 180, 217],
                    [1, 115, 166],
                    [1, 84, 121],
                    [1, 51, 67],
                    [1, 16, 6],
                ],
            ],
        ],
    ],
];

// ---------------------------------------------------------------------------
// Tables and constants for token (coefficient) decoding (transcribed from spec
// §§6.4.24-6.4.26, §§9.3.1-9.3.2, §§10.2-10.3).
//
// The values in this section were mechanically extracted by script from the spec PDF
// text extracted with `pdftotext -layout` (regex extraction via `grep -oE`), avoiding
// manual transcription errors (see the M2 handoff notes in README.md for details).
// ---------------------------------------------------------------------------

/// `token` syntax element values (spec §6.4.24).
pub const ZERO_TOKEN: u8 = 0;
pub const ONE_TOKEN: u8 = 1;
pub const TWO_TOKEN: u8 = 2;
pub const THREE_TOKEN: u8 = 3;
pub const FOUR_TOKEN: u8 = 4;
pub const DCT_VAL_CATEGORY1: u8 = 5;
pub const DCT_VAL_CATEGORY2: u8 = 6;
pub const DCT_VAL_CATEGORY3: u8 = 7;
pub const DCT_VAL_CATEGORY4: u8 = 8;
pub const DCT_VAL_CATEGORY5: u8 = 9;
pub const DCT_VAL_CATEGORY6: u8 = 10;

/// `token_tree[ 20 ]` (spec §9.3.1).
pub const TOKEN_TREE: [i32; 20] = [
    -(ZERO_TOKEN as i32),
    2,
    -(ONE_TOKEN as i32),
    4,
    6,
    10,
    -(TWO_TOKEN as i32),
    8,
    -(THREE_TOKEN as i32),
    -(FOUR_TOKEN as i32),
    12,
    14,
    -(DCT_VAL_CATEGORY1 as i32),
    -(DCT_VAL_CATEGORY2 as i32),
    16,
    18,
    -(DCT_VAL_CATEGORY3 as i32),
    -(DCT_VAL_CATEGORY4 as i32),
    -(DCT_VAL_CATEGORY5 as i32),
    -(DCT_VAL_CATEGORY6 as i32),
];

/// `binary_tree[ 2 ]` (spec §9.3.1). Represents a single bool (e.g. `more_coefs`) as a
/// tree structure. `read_tree(&BINARY_TREE, |_| prob)` is equivalent to
/// `read_bool(prob) as i32`.
pub const BINARY_TREE: [i32; 2] = [0, -1];

/// `ss_size_lookup[ BLOCK_SIZES ][ 2 ][ 2 ]` (spec §6.4.23).
/// `get_plane_block_size( subsize, plane )` = `SS_SIZE_LOOKUP[subsize][subx][suby]`.
#[rustfmt::skip]
pub const SS_SIZE_LOOKUP: [[[u8; 2]; 2]; 13] = [
    [[BLOCK_4X4,   BLOCK_INVALID], [BLOCK_INVALID, BLOCK_INVALID]],
    [[BLOCK_4X8,   BLOCK_4X4],     [BLOCK_INVALID, BLOCK_INVALID]],
    [[BLOCK_8X4,   BLOCK_INVALID], [BLOCK_4X4,     BLOCK_INVALID]],
    [[BLOCK_8X8,   BLOCK_8X4],     [BLOCK_4X8,     BLOCK_4X4]],
    [[BLOCK_8X16,  BLOCK_8X8],     [BLOCK_INVALID, BLOCK_4X8]],
    [[BLOCK_16X8,  BLOCK_INVALID], [BLOCK_8X8,     BLOCK_8X4]],
    [[BLOCK_16X16, BLOCK_16X8],    [BLOCK_8X16,    BLOCK_8X8]],
    [[BLOCK_16X32, BLOCK_16X16],   [BLOCK_INVALID, BLOCK_8X16]],
    [[BLOCK_32X16, BLOCK_INVALID], [BLOCK_16X16,   BLOCK_16X8]],
    [[BLOCK_32X32, BLOCK_32X16],   [BLOCK_16X32,   BLOCK_16X16]],
    [[BLOCK_32X64, BLOCK_32X32],   [BLOCK_INVALID, BLOCK_16X32]],
    [[BLOCK_64X32, BLOCK_INVALID], [BLOCK_32X32,   BLOCK_32X16]],
    [[BLOCK_64X64, BLOCK_64X32],   [BLOCK_32X64,   BLOCK_32X32]],
];

/// `coefband_4x4[ 16 ]` (spec §10.2).
pub const COEFBAND_4X4: [u8; 16] = [0, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 5, 5, 5];

/// Alternate implementation of `coefband_8x8plus[ 1024 ]` (spec §10.2).
///
/// Confirmed from the `pdftotext` extraction result that only the first 21 elements
/// (indices 0..=20) of the extracted table have non-trivial values
/// (0,1,1,2,2,2,3,3,3,3,4x11), and everything after that (indices 21..1024) is 5
/// (1003 of the 1024 elements are 5).
/// Instead of transcribing the 1024-element array verbatim, this structure is
/// implemented as a formula.
#[inline]
pub fn coefband_8x8plus(c: usize) -> u8 {
    const HEAD: [u8; 21] = [
        0, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    ];
    if c < HEAD.len() {
        HEAD[c]
    } else {
        5
    }
}

/// `energy_class[ 12 ]` (spec §6.4.24). `TokenCache[ pos ] = energy_class[ token ]`.
pub const ENERGY_CLASS: [u8; 12] = [0, 1, 2, 3, 3, 4, 4, 5, 5, 5, 5, 5];

/// `extra_bits[ 11 ][ 3 ]` (spec §6.4.26). `[token]` = `[cat, numExtra, coefBase]`.
#[rustfmt::skip]
pub const EXTRA_BITS: [[u8; 3]; 11] = [
    [0, 0, 0],
    [0, 0, 1],
    [0, 0, 2],
    [0, 0, 3],
    [0, 0, 4],
    [1, 1, 5],
    [2, 2, 7],
    [3, 3, 11],
    [4, 4, 19],
    [5, 5, 35],
    [6, 14, 67],
];

/// `cat_probs[ 7 ][ 14 ]` (spec §6.4.26). The actual row length matches `numExtra`
/// (`EXTRA_BITS[token][1]`); elements beyond that are unused padding per the spec
/// (zero-filled).
#[rustfmt::skip]
pub const CAT_PROBS: [[u8; 14]; 7] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [159, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [165, 145, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [173, 148, 140, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [176, 155, 140, 135, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [180, 157, 141, 134, 130, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [254, 254, 254, 252, 249, 243, 230, 196, 177, 153, 140, 133, 130, 129],
];

/// `mode2txfm_map` (spec §6.4.25). Conversion from intra prediction mode
/// (`DC_PRED`..`TM_PRED`) to `TxType`. The entries for inter prediction modes
/// (`NEARESTMV` etc., all `DCT_DCT`) are not transcribed since they are unreachable in
/// M2 (intra only).
/// `mode2txfm_map[ MB_MODE_COUNT ]` (spec §6.4.25, §10.2). Accepts intra modes (0..=9)
/// plus inter modes ([`NEARESTMV`]..[`NEWMV`], 10..=13), all returning `DCT_DCT`
/// (the spec's `mode2txfm_map` table is also defined for inter values of `y_mode`).
pub fn mode2txfm_map(mode: u8) -> crate::transform::TxType {
    use crate::transform::TxType;
    match mode {
        DC_PRED => TxType::DctDct,
        V_PRED => TxType::AdstDct,
        H_PRED => TxType::DctAdst,
        D45_PRED => TxType::DctDct,
        D135_PRED => TxType::AdstAdst,
        D117_PRED => TxType::AdstDct,
        D153_PRED => TxType::DctAdst,
        D207_PRED => TxType::DctAdst,
        D63_PRED => TxType::AdstDct,
        TM_PRED => TxType::AdstAdst,
        NEARESTMV | NEARMV | ZEROMV | NEWMV => TxType::DctDct,
        _ => unreachable!("mode2txfm_map only accepts 0..=13 (intra 0..=9 + inter 10..=13)"),
    }
}

/// `pareto_table[ 128 ][ 8 ]` (spec §10.3 "Pareto probability table").
#[rustfmt::skip]
pub const PARETO_TABLE: [[u8; 8]; 128] = [
    [3, 86, 128, 6, 86, 23, 88, 29],
    [9, 86, 129, 17, 88, 61, 94, 76],
    [15, 87, 129, 28, 89, 93, 100, 110],
    [20, 88, 130, 38, 91, 118, 106, 136],
    [26, 89, 131, 48, 92, 139, 111, 156],
    [31, 90, 131, 58, 94, 156, 117, 171],
    [37, 90, 132, 66, 95, 171, 122, 184],
    [42, 91, 132, 75, 97, 183, 127, 194],
    [47, 92, 133, 83, 98, 193, 132, 202],
    [52, 93, 133, 90, 100, 201, 137, 208],
    [57, 94, 134, 98, 101, 208, 142, 214],
    [62, 94, 135, 105, 103, 214, 146, 218],
    [66, 95, 135, 111, 104, 219, 151, 222],
    [71, 96, 136, 117, 106, 224, 155, 225],
    [76, 97, 136, 123, 107, 227, 159, 228],
    [80, 98, 137, 129, 109, 231, 162, 231],
    [84, 98, 138, 134, 110, 234, 166, 233],
    [89, 99, 138, 140, 112, 236, 170, 235],
    [93, 100, 139, 145, 113, 238, 173, 236],
    [97, 101, 140, 149, 115, 240, 176, 238],
    [101, 102, 140, 154, 116, 242, 179, 239],
    [105, 103, 141, 158, 118, 243, 182, 240],
    [109, 104, 141, 162, 119, 244, 185, 241],
    [113, 104, 142, 166, 120, 245, 187, 242],
    [116, 105, 143, 170, 122, 246, 190, 243],
    [120, 106, 143, 173, 123, 247, 192, 244],
    [123, 107, 144, 177, 125, 248, 195, 244],
    [127, 108, 145, 180, 126, 249, 197, 245],
    [130, 109, 145, 183, 128, 249, 199, 245],
    [134, 110, 146, 186, 129, 250, 201, 246],
    [137, 111, 147, 189, 131, 251, 203, 246],
    [140, 112, 147, 192, 132, 251, 205, 247],
    [143, 113, 148, 194, 133, 251, 207, 247],
    [146, 114, 149, 197, 135, 252, 208, 248],
    [149, 115, 149, 199, 136, 252, 210, 248],
    [152, 115, 150, 201, 138, 252, 211, 248],
    [155, 116, 151, 204, 139, 253, 213, 249],
    [158, 117, 151, 206, 140, 253, 214, 249],
    [161, 118, 152, 208, 142, 253, 216, 249],
    [163, 119, 153, 210, 143, 253, 217, 249],
    [166, 120, 153, 212, 144, 254, 218, 250],
    [168, 121, 154, 213, 146, 254, 220, 250],
    [171, 122, 155, 215, 147, 254, 221, 250],
    [173, 123, 155, 217, 148, 254, 222, 250],
    [176, 124, 156, 218, 150, 254, 223, 250],
    [178, 125, 157, 220, 151, 254, 224, 251],
    [180, 126, 157, 221, 152, 254, 225, 251],
    [183, 127, 158, 222, 153, 254, 226, 251],
    [185, 128, 159, 224, 155, 255, 227, 251],
    [187, 129, 160, 225, 156, 255, 228, 251],
    [189, 131, 160, 226, 157, 255, 228, 251],
    [191, 132, 161, 227, 159, 255, 229, 251],
    [193, 133, 162, 228, 160, 255, 230, 252],
    [195, 134, 163, 230, 161, 255, 231, 252],
    [197, 135, 163, 231, 162, 255, 231, 252],
    [199, 136, 164, 232, 163, 255, 232, 252],
    [201, 137, 165, 233, 165, 255, 233, 252],
    [202, 138, 166, 233, 166, 255, 233, 252],
    [204, 139, 166, 234, 167, 255, 234, 252],
    [206, 140, 167, 235, 168, 255, 235, 252],
    [207, 141, 168, 236, 169, 255, 235, 252],
    [209, 142, 169, 237, 171, 255, 236, 252],
    [210, 144, 169, 237, 172, 255, 236, 252],
    [212, 145, 170, 238, 173, 255, 237, 252],
    [214, 146, 171, 239, 174, 255, 237, 253],
    [215, 147, 172, 240, 175, 255, 238, 253],
    [216, 148, 173, 240, 176, 255, 238, 253],
    [218, 149, 173, 241, 177, 255, 239, 253],
    [219, 150, 174, 241, 179, 255, 239, 253],
    [220, 152, 175, 242, 180, 255, 240, 253],
    [222, 153, 176, 242, 181, 255, 240, 253],
    [223, 154, 177, 243, 182, 255, 240, 253],
    [224, 155, 178, 244, 183, 255, 241, 253],
    [225, 156, 178, 244, 184, 255, 241, 253],
    [226, 158, 179, 244, 185, 255, 242, 253],
    [228, 159, 180, 245, 186, 255, 242, 253],
    [229, 160, 181, 245, 187, 255, 242, 253],
    [230, 161, 182, 246, 188, 255, 243, 253],
    [231, 163, 183, 246, 189, 255, 243, 253],
    [232, 164, 184, 247, 190, 255, 243, 253],
    [233, 165, 185, 247, 191, 255, 244, 253],
    [234, 166, 185, 247, 192, 255, 244, 253],
    [235, 168, 186, 248, 193, 255, 244, 253],
    [236, 169, 187, 248, 194, 255, 244, 253],
    [236, 170, 188, 248, 195, 255, 245, 253],
    [237, 171, 189, 249, 196, 255, 245, 254],
    [238, 173, 190, 249, 197, 255, 245, 254],
    [239, 174, 191, 249, 198, 255, 245, 254],
    [240, 175, 192, 249, 199, 255, 246, 254],
    [240, 177, 193, 250, 200, 255, 246, 254],
    [241, 178, 194, 250, 201, 255, 246, 254],
    [242, 179, 195, 250, 202, 255, 246, 254],
    [242, 181, 196, 250, 203, 255, 247, 254],
    [243, 182, 197, 251, 204, 255, 247, 254],
    [244, 184, 198, 251, 205, 255, 247, 254],
    [244, 185, 199, 251, 206, 255, 247, 254],
    [245, 186, 200, 251, 207, 255, 247, 254],
    [246, 188, 201, 252, 207, 255, 248, 254],
    [246, 189, 202, 252, 208, 255, 248, 254],
    [247, 191, 203, 252, 209, 255, 248, 254],
    [247, 192, 204, 252, 210, 255, 248, 254],
    [248, 194, 205, 252, 211, 255, 248, 254],
    [248, 195, 206, 252, 212, 255, 249, 254],
    [249, 197, 207, 253, 213, 255, 249, 254],
    [249, 198, 208, 253, 214, 255, 249, 254],
    [250, 200, 210, 253, 215, 255, 249, 254],
    [250, 201, 211, 253, 215, 255, 249, 254],
    [250, 203, 212, 253, 216, 255, 249, 254],
    [251, 204, 213, 253, 217, 255, 250, 254],
    [251, 206, 214, 254, 218, 255, 250, 254],
    [252, 207, 216, 254, 219, 255, 250, 254],
    [252, 209, 217, 254, 220, 255, 250, 254],
    [252, 211, 218, 254, 221, 255, 250, 254],
    [253, 213, 219, 254, 222, 255, 250, 254],
    [253, 214, 221, 254, 223, 255, 250, 254],
    [253, 216, 222, 254, 224, 255, 251, 254],
    [253, 218, 224, 254, 225, 255, 251, 254],
    [254, 220, 225, 254, 225, 255, 251, 254],
    [254, 222, 227, 255, 226, 255, 251, 254],
    [254, 224, 228, 255, 227, 255, 251, 254],
    [254, 226, 230, 255, 228, 255, 251, 254],
    [255, 228, 231, 255, 230, 255, 251, 254],
    [255, 230, 233, 255, 231, 255, 252, 254],
    [255, 232, 235, 255, 232, 255, 252, 254],
    [255, 235, 237, 255, 233, 255, 252, 254],
    [255, 238, 240, 255, 235, 255, 252, 255],
    [255, 241, 243, 255, 236, 255, 252, 254],
    [255, 246, 247, 255, 239, 255, 253, 255],
];

/// Precomputed `pareto(node, prob)` table for `prob in 1..=255` and `node - 2 in 0..8`,
/// turning `pareto()` into a branchless table lookup in the token-decoding hot loop.
const PARETO_FULL: [[u8; 8]; 256] = {
    let mut out = [[0u8; 8]; 256];
    let mut prob = 1usize;
    while prob < 256 {
        let x = (prob - 1) >> 1;
        let mut k = 0usize;
        while k < 8 {
            out[prob][k] = if prob & 1 == 1 {
                PARETO_TABLE[x][k]
            } else {
                ((PARETO_TABLE[x][k] as u32 + PARETO_TABLE[x + 1][k] as u32) >> 1) as u8
            };
            k += 1;
        }
        prob += 1;
    }
    out
};

/// `pareto( node, prob )` (spec §9.3.2). Used to select the probability for the
/// `token` syntax element. Returns `prob` unmodified when `node < 2`; otherwise looks
/// up [`PARETO_TABLE`] (averaging the two adjacent rows when `prob` is even).
#[inline]
pub fn pareto(node: usize, prob: u8) -> u8 {
    if node < 2 {
        return prob;
    }
    PARETO_FULL[prob as usize][node - 2]
}

// ---------------------------------------------------------------------------
// Default probability tables and conversion tables needed for inter frames (M3)
// (transcribed from spec §10.2, §10.5).
// ---------------------------------------------------------------------------

/// `size_group_lookup[ BLOCK_SIZES ]` (spec §10.2). Used for probability selection
/// (`y_mode_probs[ctx]`) of `intra_mode`/`sub_intra_mode` in non-keyframes.
pub const SIZE_GROUP_LOOKUP: [u8; 13] = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3];

/// `default_partition_probs[ PARTITION_CONTEXTS ][ PARTITION_TYPES - 1 ]` (spec §10.5).
/// Separate from the keyframe [`KF_PARTITION_PROBS`], this probability table is updated
/// by `compressed_header` in non-keyframes.
pub const DEFAULT_PARTITION_PROBS: [[u8; 3]; 16] = [
    [199, 122, 141],
    [147, 63, 159],
    [148, 133, 118],
    [121, 104, 114],
    [174, 73, 87],
    [92, 41, 83],
    [82, 99, 50],
    [53, 39, 39],
    [177, 58, 59],
    [68, 26, 63],
    [52, 79, 25],
    [17, 14, 12],
    [222, 34, 30],
    [72, 16, 44],
    [58, 32, 12],
    [10, 7, 6],
];

/// `default_y_mode_probs[ BLOCK_SIZE_GROUPS ][ INTRA_MODES - 1 ]` (spec §10.5).
/// Used for `intra_mode`/`sub_intra_mode` (`y_mode_probs`) in non-keyframes.
/// Unlike `default_uv_mode_probs`, this is updated by `compressed_header`'s
/// `read_y_mode_probs()`.
pub const DEFAULT_Y_MODE_PROBS: [[u8; 9]; 4] = [
    [65, 32, 18, 144, 162, 194, 41, 51, 98],
    [132, 68, 18, 165, 217, 196, 45, 40, 78],
    [173, 80, 19, 176, 240, 193, 64, 35, 46],
    [221, 135, 38, 194, 248, 121, 96, 85, 29],
];

/// `default_uv_mode_probs[ INTRA_MODES ][ INTRA_MODES - 1 ]` (spec §10.5).
/// `compressed_header` has no syntax to update this, so this default value is always
/// used (see `compressed_header()` in spec §6.3: `read_y_mode_probs()` updates only
/// `y_mode_probs`, and there is no syntax to update `uv_mode_probs`).
pub const DEFAULT_UV_MODE_PROBS: [[u8; 9]; 10] = [
    [120, 7, 76, 176, 208, 126, 28, 54, 103],
    [48, 12, 154, 155, 139, 90, 34, 117, 119],
    [67, 6, 25, 204, 243, 158, 13, 21, 96],
    [97, 5, 44, 131, 176, 139, 48, 68, 97],
    [83, 5, 42, 156, 111, 152, 26, 49, 152],
    [80, 5, 58, 178, 74, 83, 33, 62, 145],
    [86, 5, 32, 154, 192, 168, 14, 22, 163],
    [85, 5, 32, 156, 216, 148, 19, 29, 73],
    [77, 7, 64, 116, 132, 122, 37, 126, 120],
    [101, 21, 107, 181, 192, 103, 19, 67, 125],
];

/// `default_is_inter_prob[ IS_INTER_CONTEXTS ]` (spec §10.5).
pub const DEFAULT_IS_INTER_PROB: [u8; 4] = [9, 102, 187, 225];

/// `default_comp_mode_prob[ COMP_MODE_CONTEXTS ]` (spec §10.5).
pub const DEFAULT_COMP_MODE_PROB: [u8; 5] = [239, 183, 119, 96, 41];

/// `default_comp_ref_prob[ REF_CONTEXTS ]` (spec §10.5).
pub const DEFAULT_COMP_REF_PROB: [u8; 5] = [50, 126, 123, 221, 226];

/// `default_single_ref_prob[ REF_CONTEXTS ][ 2 ]` (spec §10.5).
pub const DEFAULT_SINGLE_REF_PROB: [[u8; 2]; 5] =
    [[33, 16], [77, 74], [142, 142], [172, 170], [238, 247]];

/// `default_inter_mode_probs[ INTER_MODE_CONTEXTS ][ INTER_MODES - 1 ]` (spec §10.5).
pub const DEFAULT_INTER_MODE_PROBS: [[u8; 3]; 7] = [
    [2, 173, 34],
    [7, 145, 85],
    [7, 166, 63],
    [7, 94, 66],
    [8, 64, 46],
    [17, 81, 31],
    [25, 29, 30],
];

/// `default_interp_filter_probs[ INTERP_FILTER_CONTEXTS ][ SWITCHABLE_FILTERS - 1 ]`
/// (spec §10.5).
pub const DEFAULT_INTERP_FILTER_PROBS: [[u8; 2]; 4] = [[235, 162], [36, 255], [34, 3], [149, 144]];

/// `default_mv_joint_probs[ 3 ]` (spec §10.5).
pub const DEFAULT_MV_JOINT_PROBS: [u8; 3] = [32, 64, 96];

/// `default_mv_sign_prob[ 2 ]` (spec §10.5).
pub const DEFAULT_MV_SIGN_PROB: [u8; 2] = [128, 128];

/// `default_mv_class_probs[ 2 ][ MV_CLASSES - 1 ]` (spec §10.5).
pub const DEFAULT_MV_CLASS_PROBS: [[u8; 10]; 2] = [
    [224, 144, 192, 168, 192, 176, 192, 198, 198, 245],
    [216, 128, 176, 160, 176, 176, 192, 198, 198, 208],
];

/// `default_mv_class0_bit_prob[ 2 ]` (spec §10.5).
pub const DEFAULT_MV_CLASS0_BIT_PROB: [u8; 2] = [216, 208];

/// `default_mv_bits_prob[ 2 ][ MV_OFFSET_BITS ]` (spec §10.5).
pub const DEFAULT_MV_BITS_PROB: [[u8; 10]; 2] = [
    [136, 140, 148, 160, 176, 192, 224, 234, 234, 240],
    [136, 140, 148, 160, 176, 192, 224, 234, 234, 240],
];

/// `default_mv_class0_fr_probs[ 2 ][ CLASS0_SIZE ][ 3 ]` (spec §10.5).
pub const DEFAULT_MV_CLASS0_FR_PROBS: [[[u8; 3]; 2]; 2] = [
    [[128, 128, 64], [96, 112, 64]],
    [[128, 128, 64], [96, 112, 64]],
];

/// `default_mv_fr_probs[ 2 ][ 3 ]` (spec §10.5).
pub const DEFAULT_MV_FR_PROBS: [[u8; 3]; 2] = [[64, 96, 64], [64, 96, 64]];

/// `default_mv_class0_hp_prob[ 2 ]` (spec §10.5).
pub const DEFAULT_MV_CLASS0_HP_PROB: [u8; 2] = [160, 160];

/// `default_mv_hp_prob[ 2 ]` (spec §10.5).
pub const DEFAULT_MV_HP_PROB: [u8; 2] = [128, 128];

// =============================================================================
// Probability adaptation (spec §8.4 "Probability adaptation process").
// =============================================================================

/// `COUNT_SAT` (spec §3, constant used in probability adaptation).
pub const COUNT_SAT: u32 = 20;
/// `MAX_UPDATE_FACTOR` (spec §3).
pub const MAX_UPDATE_FACTOR: u32 = 128;

/// `small_token_tree[ 6 ]` (spec §8.4.3). Used when applying `merge_probs` to
/// `coef_probs[...][ 1..3 ]` (a 3-value tree combining `ONE_TOKEN`/`TWO_TOKEN` and above).
pub const SMALL_TOKEN_TREE: [i32; 6] = [
    0,
    0, // unused (index 0..1; merge_probs traverses from i=2, so these are never referenced)
    -(ZERO_TOKEN as i32),
    4,
    -(ONE_TOKEN as i32),
    -(TWO_TOKEN as i32),
];
