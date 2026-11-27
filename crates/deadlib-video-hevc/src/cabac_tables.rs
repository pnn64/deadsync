//! CABAC engine tables (HEVC spec section 9.3).
//!
//! `LPS_RANGE` and `MLPS_STATE` are `rangeTabLps` and the `transIdx`
//! state transitions, inherited verbatim from H.264 and kept in the packed
//! FFmpeg-style layout; `CABAC_FUSED` combines them into the single table
//! the decoder reads per bin. `INIT_VALUES` holds the per-context init
//! values (spec tables 9-5 to 9-37) for the three init types.

/// Equivalent of HEVC spec `rangeTabLPS` (table 9-41), packed for the
/// FFmpeg-style 4-way range bucket lookup (`(range >> 6) & 3`).
#[rustfmt::skip]
pub const LPS_RANGE: [u8; 512] = [
    // Range group 0
    128, 128, 128, 128, 128, 128, 123, 123,
    116, 116, 111, 111, 105, 105, 100, 100,
     95,  95,  90,  90,  85,  85,  81,  81,
     77,  77,  73,  73,  69,  69,  66,  66,
     62,  62,  59,  59,  56,  56,  53,  53,
     51,  51,  48,  48,  46,  46,  43,  43,
     41,  41,  39,  39,  37,  37,  35,  35,
     33,  33,  32,  32,  30,  30,  29,  29,
     27,  27,  26,  26,  24,  24,  23,  23,
     22,  22,  21,  21,  20,  20,  19,  19,
     18,  18,  17,  17,  16,  16,  15,  15,
     14,  14,  14,  14,  13,  13,  12,  12,
     12,  12,  11,  11,  11,  11,  10,  10,
     10,  10,   9,   9,   9,   9,   8,   8,
      8,   8,   7,   7,   7,   7,   7,   7,
      6,   6,   6,   6,   6,   6,   2,   2,
    // Range group 1
    176, 176, 167, 167, 158, 158, 150, 150,
    142, 142, 135, 135, 128, 128, 122, 122,
    116, 116, 110, 110, 104, 104,  99,  99,
     94,  94,  89,  89,  85,  85,  80,  80,
     76,  76,  72,  72,  69,  69,  65,  65,
     62,  62,  59,  59,  56,  56,  53,  53,
     50,  50,  48,  48,  45,  45,  43,  43,
     41,  41,  39,  39,  37,  37,  35,  35,
     33,  33,  31,  31,  30,  30,  28,  28,
     27,  27,  26,  26,  24,  24,  23,  23,
     22,  22,  21,  21,  20,  20,  19,  19,
     18,  18,  17,  17,  16,  16,  15,  15,
     14,  14,  14,  14,  13,  13,  12,  12,
     12,  12,  11,  11,  11,  11,  10,  10,
      9,   9,   9,   9,   9,   9,   8,   8,
      8,   8,   7,   7,   7,   7,   2,   2,
    // Range group 2
    208, 208, 197, 197, 187, 187, 178, 178,
    169, 169, 160, 160, 152, 152, 144, 144,
    137, 137, 130, 130, 123, 123, 117, 117,
    111, 111, 105, 105, 100, 100,  95,  95,
     90,  90,  86,  86,  81,  81,  77,  77,
     73,  73,  69,  69,  66,  66,  63,  63,
     59,  59,  56,  56,  54,  54,  51,  51,
     48,  48,  46,  46,  43,  43,  41,  41,
     39,  39,  37,  37,  35,  35,  33,  33,
     32,  32,  30,  30,  29,  29,  27,  27,
     26,  26,  25,  25,  23,  23,  22,  22,
     21,  21,  20,  20,  19,  19,  18,  18,
     17,  17,  16,  16,  15,  15,  15,  15,
     14,  14,  13,  13,  12,  12,  12,  12,
     11,  11,  11,  11,  10,  10,  10,  10,
      9,   9,   9,   9,   8,   8,   2,   2,
    // Range group 3
    240, 240, 227, 227, 216, 216, 205, 205,
    195, 195, 185, 185, 175, 175, 166, 166,
    158, 158, 150, 150, 142, 142, 135, 135,
    128, 128, 122, 122, 116, 116, 110, 110,
    104, 104,  99,  99,  94,  94,  89,  89,
     85,  85,  80,  80,  76,  76,  72,  72,
     69,  69,  65,  65,  62,  62,  59,  59,
     56,  56,  53,  53,  50,  50,  48,  48,
     45,  45,  43,  43,  41,  41,  39,  39,
     37,  37,  35,  35,  33,  33,  31,  31,
     30,  30,  28,  28,  27,  27,  25,  25,
     24,  24,  23,  23,  22,  22,  21,  21,
     20,  20,  19,  19,  18,  18,  17,  17,
     16,  16,  15,  15,  14,  14,  14,  14,
     13,  13,  12,  12,  12,  12,  11,  11,
     11,  11,  10,  10,   9,   9,   2,   2,
];

/// Combined MPS / LPS state transition table. Indices 0..127 are the MPS
/// branch (next state after decoding the most-probable symbol); indices
/// 128..255 are the LPS branch.
#[rustfmt::skip]
pub const MLPS_STATE: [u8; 256] = [
    // MPS transitions
    127, 126,  77,  76,  77,  76,  75,  74,
     75,  74,  75,  74,  73,  72,  73,  72,
     73,  72,  71,  70,  71,  70,  71,  70,
     69,  68,  69,  68,  67,  66,  67,  66,
     67,  66,  65,  64,  65,  64,  63,  62,
     61,  60,  61,  60,  61,  60,  59,  58,
     59,  58,  57,  56,  55,  54,  55,  54,
     53,  52,  53,  52,  51,  50,  49,  48,
     49,  48,  47,  46,  45,  44,  45,  44,
     43,  42,  43,  42,  39,  38,  39,  38,
     37,  36,  37,  36,  33,  32,  33,  32,
     31,  30,  31,  30,  27,  26,  27,  26,
     25,  24,  23,  22,  23,  22,  19,  18,
     19,  18,  17,  16,  15,  14,  13,  12,
     11,  10,   9,   8,   9,   8,   5,   4,
      5,   4,   3,   2,   1,   0,   0,   1,
    // LPS transitions
      2,   3,   4,   5,   6,   7,   8,   9,
     10,  11,  12,  13,  14,  15,  16,  17,
     18,  19,  20,  21,  22,  23,  24,  25,
     26,  27,  28,  29,  30,  31,  32,  33,
     34,  35,  36,  37,  38,  39,  40,  41,
     42,  43,  44,  45,  46,  47,  48,  49,
     50,  51,  52,  53,  54,  55,  56,  57,
     58,  59,  60,  61,  62,  63,  64,  65,
     66,  67,  68,  69,  70,  71,  72,  73,
     74,  75,  76,  77,  78,  79,  80,  81,
     82,  83,  84,  85,  86,  87,  88,  89,
     90,  91,  92,  93,  94,  95,  96,  97,
     98,  99, 100, 101, 102, 103, 104, 105,
    106, 107, 108, 109, 110, 111, 112, 113,
    114, 115, 116, 117, 118, 119, 120, 121,
    122, 123, 124, 125, 124, 125, 126, 127,
];

// ---- Per-syntax-element CABAC context init values (HEVC spec table 9-4) ----
//
// `INIT_VALUES[init_type][i]` is the 8-bit packed init value for context `i`,
// where `init_type` is 0 for I slices, 1 for "P with cabac_init=0 / B with
// cabac_init=1", and 2 for "P with cabac_init=1 / B with cabac_init=0".
// Spec encoding: high nibble = `slopeIdx`, low nibble = `offsetIdx`.
//
// Layout matches FFmpeg's `libavcodec/hevc/cabac.c` `init_values[]`. Contexts
// that aren't read in a given slice type are filled with `CNU` (= 154,
// "context not used") so the indexing stays uniform across slice types.
//
// Total: 179 contexts. The named offsets/lengths in [`ctx`] below partition
// the table by syntax element. Sub-element ordering matches HEVC spec table
// 9-4 / 9-5 / etc. exactly.

/// Number of CABAC contexts in HEVC.
/// `LPS_RANGE` and both `MLPS_STATE` transitions fused into one lookup,
/// indexed by `(state << 2) | quartile` with `quartile = (range >> 6) & 3`:
/// bits 0..8 hold rangeTabLps, bits 8..16 the next state after an MPS and
/// bits 16..24 the next state after an LPS. One load per bin instead of two
/// dependent ones; the 1024 entries cover every `u8` state, so the index
/// needs no bounds check (states 128..=255 never occur and mirror 0..=127).
pub static CABAC_FUSED: [u32; 1024] = build_cabac_fused();

const fn build_cabac_fused() -> [u32; 1024] {
    let mut t = [0u32; 1024];
    let mut i = 0;
    while i < 1024 {
        let s = (i >> 2) & 127;
        let q = i & 3;
        let lps = LPS_RANGE[q * 128 + s] as u32;
        let next_mps = MLPS_STATE[128 + s] as u32;
        let next_lps = MLPS_STATE[127 - s] as u32;
        t[i] = lps | (next_mps << 8) | (next_lps << 16);
        i += 1;
    }
    t
}

pub const HEVC_CONTEXTS: usize = 179;

/// "Context not used" placeholder, kept so contexts unused in a given slice
/// type still have a defined value (= 154) when the table is indexed.
const CNU: u8 = 154;

#[rustfmt::skip]
pub static INIT_VALUES: [[u8; HEVC_CONTEXTS]; 3] = [
    // ---------- init_type = 0 (I slice) ----------
    [
        // sao_merge_flag
        153,
        // sao_type_idx
        200,
        // split_coding_unit_flag (3)
        139, 141, 157,
        // cu_transquant_bypass_flag (1)
        154,
        // skip_flag (3) — not read in I, CNU
        CNU, CNU, CNU,
        // cu_qp_delta (3)
        154, 154, 154,
        // pred_mode_flag (1) — not read in I, CNU
        CNU,
        // part_mode (4) — only the first context is read in I-slice
        184, CNU, CNU, CNU,
        // prev_intra_luma_pred_flag (1)
        184,
        // intra_chroma_pred_mode (2)
        63, 139,
        // merge_flag (1) — P/B only
        CNU,
        // merge_idx (1) — P/B only
        CNU,
        // inter_pred_idc (5) — B only
        CNU, CNU, CNU, CNU, CNU,
        // ref_idx_l0 (2) — P/B only
        CNU, CNU,
        // ref_idx_l1 (2) — B only
        CNU, CNU,
        // abs_mvd_greater0_flag (2) — P/B only
        CNU, CNU,
        // abs_mvd_greater1_flag (2) — P/B only
        CNU, CNU,
        // mvp_lx_flag (1) — P/B only
        CNU,
        // no_residual_data_flag (1) — P/B only
        CNU,
        // split_transform_flag (3)
        153, 138, 138,
        // cbf_luma (2)
        111, 141,
        // cbf_cb / cbf_cr (5)
        94, 138, 182, 154, 154,
        // transform_skip_flag (2) — luma + chroma
        139, 139,
        // explicit_rdpcm_flag (2)
        139, 139,
        // explicit_rdpcm_dir_flag (2)
        139, 139,
        // last_significant_coeff_x_prefix (18)
        110, 110, 124, 125, 140, 153, 125, 127, 140, 109, 111, 143, 127, 111,
         79, 108, 123,  63,
        // last_significant_coeff_y_prefix (18)
        110, 110, 124, 125, 140, 153, 125, 127, 140, 109, 111, 143, 127, 111,
         79, 108, 123,  63,
        // significant_coeff_group_flag (4)
        91, 171, 134, 141,
        // significant_coeff_flag (44)
        111, 111, 125, 110, 110,  94, 124, 108, 124, 107, 125, 141, 179, 153,
        125, 107, 125, 141, 179, 153, 125, 107, 125, 141, 179, 153, 125, 140,
        139, 182, 182, 152, 136, 152, 136, 153, 136, 139, 111, 136, 139, 111,
        141, 111,
        // coeff_abs_level_greater1_flag (24)
        140,  92, 137, 138, 140, 152, 138, 139, 153,  74, 149,  92, 139, 107,
        122, 152, 140, 179, 166, 182, 140, 227, 122, 197,
        // coeff_abs_level_greater2_flag (6)
        138, 153, 136, 167, 152, 152,
        // log2_res_scale_abs (8)
        154, 154, 154, 154, 154, 154, 154, 154,
        // res_scale_sign_flag (2)
        154, 154,
        // cu_chroma_qp_offset_flag (1)
        154,
        // cu_chroma_qp_offset_idx (1)
        154,
    ],
    // ---------- init_type = 1 (P cabac_init=0 / B cabac_init=1) ----------
    [
        // sao_merge_flag
        153,
        // sao_type_idx
        185,
        // split_coding_unit_flag
        107, 139, 126,
        // cu_transquant_bypass_flag
        154,
        // skip_flag
        197, 185, 201,
        // cu_qp_delta
        154, 154, 154,
        // pred_mode_flag
        149,
        // part_mode
        154, 139, 154, 154,
        // prev_intra_luma_pred_flag
        154,
        // intra_chroma_pred_mode
        152, 139,
        // merge_flag
        110,
        // merge_idx
        122,
        // inter_pred_idc
        95, 79, 63, 31, 31,
        // ref_idx_l0
        153, 153,
        // ref_idx_l1
        153, 153,
        // abs_mvd_greater0_flag
        140, 198,
        // abs_mvd_greater1_flag
        140, 198,
        // mvp_lx_flag
        168,
        // no_residual_data_flag
        79,
        // split_transform_flag
        124, 138,  94,
        // cbf_luma
        153, 111,
        // cbf_cb / cbf_cr
        149, 107, 167, 154, 154,
        // transform_skip_flag
        139, 139,
        // explicit_rdpcm_flag
        139, 139,
        // explicit_rdpcm_dir_flag
        139, 139,
        // last_significant_coeff_x_prefix
        125, 110,  94, 110,  95,  79, 125, 111, 110,  78, 110, 111, 111,  95,
         94, 108, 123, 108,
        // last_significant_coeff_y_prefix
        125, 110,  94, 110,  95,  79, 125, 111, 110,  78, 110, 111, 111,  95,
         94, 108, 123, 108,
        // significant_coeff_group_flag
        121, 140,  61, 154,
        // significant_coeff_flag
        155, 154, 139, 153, 139, 123, 123,  63, 153, 166, 183, 140, 136, 153,
        154, 166, 183, 140, 136, 153, 154, 166, 183, 140, 136, 153, 154, 170,
        153, 123, 123, 107, 121, 107, 121, 167, 151, 183, 140, 151, 183, 140,
        140, 140,
        // coeff_abs_level_greater1_flag
        154, 196, 196, 167, 154, 152, 167, 182, 182, 134, 149, 136, 153, 121,
        136, 137, 169, 194, 166, 167, 154, 167, 137, 182,
        // coeff_abs_level_greater2_flag
        107, 167,  91, 122, 107, 167,
        // log2_res_scale_abs
        154, 154, 154, 154, 154, 154, 154, 154,
        // res_scale_sign_flag
        154, 154,
        // cu_chroma_qp_offset_flag
        154,
        // cu_chroma_qp_offset_idx
        154,
    ],
    // ---------- init_type = 2 (P cabac_init=1 / B cabac_init=0) ----------
    [
        // sao_merge_flag
        153,
        // sao_type_idx
        160,
        // split_coding_unit_flag
        107, 139, 126,
        // cu_transquant_bypass_flag
        154,
        // skip_flag
        197, 185, 201,
        // cu_qp_delta
        154, 154, 154,
        // pred_mode_flag
        134,
        // part_mode
        154, 139, 154, 154,
        // prev_intra_luma_pred_flag
        183,
        // intra_chroma_pred_mode
        152, 139,
        // merge_flag
        154,
        // merge_idx
        137,
        // inter_pred_idc
        95, 79, 63, 31, 31,
        // ref_idx_l0
        153, 153,
        // ref_idx_l1
        153, 153,
        // abs_mvd_greater0_flag
        169, 198,
        // abs_mvd_greater1_flag
        169, 198,
        // mvp_lx_flag
        168,
        // no_residual_data_flag
        79,
        // split_transform_flag
        224, 167, 122,
        // cbf_luma
        153, 111,
        // cbf_cb / cbf_cr
        149,  92, 167, 154, 154,
        // transform_skip_flag
        139, 139,
        // explicit_rdpcm_flag
        139, 139,
        // explicit_rdpcm_dir_flag
        139, 139,
        // last_significant_coeff_x_prefix
        125, 110, 124, 110,  95,  94, 125, 111, 111,  79, 125, 126, 111, 111,
         79, 108, 123,  93,
        // last_significant_coeff_y_prefix
        125, 110, 124, 110,  95,  94, 125, 111, 111,  79, 125, 126, 111, 111,
         79, 108, 123,  93,
        // significant_coeff_group_flag
        121, 140,  61, 154,
        // significant_coeff_flag
        170, 154, 139, 153, 139, 123, 123,  63, 124, 166, 183, 140, 136, 153,
        154, 166, 183, 140, 136, 153, 154, 166, 183, 140, 136, 153, 154, 170,
        153, 138, 138, 122, 121, 122, 121, 167, 151, 183, 140, 151, 183, 140,
        140, 140,
        // coeff_abs_level_greater1_flag
        154, 196, 167, 167, 154, 152, 167, 182, 182, 134, 149, 136, 153, 121,
        136, 122, 169, 208, 166, 167, 154, 152, 167, 182,
        // coeff_abs_level_greater2_flag
        107, 167,  91, 107, 107, 167,
        // log2_res_scale_abs
        154, 154, 154, 154, 154, 154, 154, 154,
        // res_scale_sign_flag
        154, 154,
        // cu_chroma_qp_offset_flag
        154,
        // cu_chroma_qp_offset_idx
        154,
    ],
];

/// Per-syntax-element offsets and bin counts into [`INIT_VALUES`] /
/// `CabacContexts::state`. Mirrors the `_OFFSET` enum FFmpeg builds from its
/// `CABAC_ELEMS` macro.
///
/// Syntax elements that are pure-bypass or terminate-only (e.g. `mpm_idx`,
/// `coeff_sign_flag`, `end_of_slice_flag`, the various SAO offset bits) have
/// no entry here because they don't consume CABAC contexts.
pub mod ctx {
    pub const SAO_MERGE_FLAG: usize = 0;
    pub const SAO_MERGE_FLAG_LEN: usize = 1;

    pub const SAO_TYPE_IDX: usize = 1;
    pub const SAO_TYPE_IDX_LEN: usize = 1;

    pub const SPLIT_CODING_UNIT_FLAG: usize = 2;
    pub const SPLIT_CODING_UNIT_FLAG_LEN: usize = 3;

    pub const CU_TRANSQUANT_BYPASS_FLAG: usize = 5;
    pub const CU_TRANSQUANT_BYPASS_FLAG_LEN: usize = 1;

    pub const SKIP_FLAG: usize = 6;
    pub const SKIP_FLAG_LEN: usize = 3;

    pub const CU_QP_DELTA: usize = 9;
    pub const CU_QP_DELTA_LEN: usize = 3;

    pub const PRED_MODE_FLAG: usize = 12;
    pub const PRED_MODE_FLAG_LEN: usize = 1;

    pub const PART_MODE: usize = 13;
    pub const PART_MODE_LEN: usize = 4;

    pub const PREV_INTRA_LUMA_PRED_FLAG: usize = 17;
    pub const PREV_INTRA_LUMA_PRED_FLAG_LEN: usize = 1;

    pub const INTRA_CHROMA_PRED_MODE: usize = 18;
    pub const INTRA_CHROMA_PRED_MODE_LEN: usize = 2;

    pub const MERGE_FLAG: usize = 20;
    pub const MERGE_FLAG_LEN: usize = 1;

    pub const MERGE_IDX: usize = 21;
    pub const MERGE_IDX_LEN: usize = 1;

    pub const INTER_PRED_IDC: usize = 22;
    pub const INTER_PRED_IDC_LEN: usize = 5;

    pub const REF_IDX_L0: usize = 27;
    pub const REF_IDX_L0_LEN: usize = 2;

    pub const REF_IDX_L1: usize = 29;
    pub const REF_IDX_L1_LEN: usize = 2;

    pub const ABS_MVD_GREATER0_FLAG: usize = 31;
    pub const ABS_MVD_GREATER0_FLAG_LEN: usize = 2;

    pub const ABS_MVD_GREATER1_FLAG: usize = 33;
    pub const ABS_MVD_GREATER1_FLAG_LEN: usize = 2;

    pub const MVP_LX_FLAG: usize = 35;
    pub const MVP_LX_FLAG_LEN: usize = 1;

    pub const NO_RESIDUAL_DATA_FLAG: usize = 36;
    pub const NO_RESIDUAL_DATA_FLAG_LEN: usize = 1;

    pub const SPLIT_TRANSFORM_FLAG: usize = 37;
    pub const SPLIT_TRANSFORM_FLAG_LEN: usize = 3;

    pub const CBF_LUMA: usize = 40;
    pub const CBF_LUMA_LEN: usize = 2;

    pub const CBF_CB_CR: usize = 42;
    pub const CBF_CB_CR_LEN: usize = 5;

    pub const TRANSFORM_SKIP_FLAG: usize = 47;
    pub const TRANSFORM_SKIP_FLAG_LEN: usize = 2;

    pub const EXPLICIT_RDPCM_FLAG: usize = 49;
    pub const EXPLICIT_RDPCM_FLAG_LEN: usize = 2;

    pub const EXPLICIT_RDPCM_DIR_FLAG: usize = 51;
    pub const EXPLICIT_RDPCM_DIR_FLAG_LEN: usize = 2;

    pub const LAST_SIGNIFICANT_COEFF_X_PREFIX: usize = 53;
    pub const LAST_SIGNIFICANT_COEFF_X_PREFIX_LEN: usize = 18;

    pub const LAST_SIGNIFICANT_COEFF_Y_PREFIX: usize = 71;
    pub const LAST_SIGNIFICANT_COEFF_Y_PREFIX_LEN: usize = 18;

    pub const SIGNIFICANT_COEFF_GROUP_FLAG: usize = 89;
    pub const SIGNIFICANT_COEFF_GROUP_FLAG_LEN: usize = 4;

    pub const SIGNIFICANT_COEFF_FLAG: usize = 93;
    pub const SIGNIFICANT_COEFF_FLAG_LEN: usize = 44;

    pub const COEFF_ABS_LEVEL_GREATER1_FLAG: usize = 137;
    pub const COEFF_ABS_LEVEL_GREATER1_FLAG_LEN: usize = 24;

    pub const COEFF_ABS_LEVEL_GREATER2_FLAG: usize = 161;
    pub const COEFF_ABS_LEVEL_GREATER2_FLAG_LEN: usize = 6;

    pub const LOG2_RES_SCALE_ABS: usize = 167;
    pub const LOG2_RES_SCALE_ABS_LEN: usize = 8;

    pub const RES_SCALE_SIGN_FLAG: usize = 175;
    pub const RES_SCALE_SIGN_FLAG_LEN: usize = 2;

    pub const CU_CHROMA_QP_OFFSET_FLAG: usize = 177;
    pub const CU_CHROMA_QP_OFFSET_FLAG_LEN: usize = 1;

    pub const CU_CHROMA_QP_OFFSET_IDX: usize = 178;
    pub const CU_CHROMA_QP_OFFSET_IDX_LEN: usize = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both `INIT_VALUES` rows must contain exactly `HEVC_CONTEXTS` entries.
    /// (`[u8; HEVC_CONTEXTS]` already enforces this at compile time, so this
    /// is just defense in depth — it'll fail to compile if the table is
    /// resized accidentally.)
    #[test]
    fn test_init_values_shape() {
        for row in &INIT_VALUES {
            assert_eq!(row.len(), HEVC_CONTEXTS);
        }
    }

    /// Verify offsets cover the table without overlap and end exactly at
    /// `HEVC_CONTEXTS`. This catches transcription errors in the `ctx` module
    /// (the most likely source of subtle off-by-ones).
    #[test]
    fn test_ctx_offsets_partition_table() {
        // Pairs of (offset, length) in the order they appear in the table.
        let elements: &[(usize, usize)] = &[
            (ctx::SAO_MERGE_FLAG, ctx::SAO_MERGE_FLAG_LEN),
            (ctx::SAO_TYPE_IDX, ctx::SAO_TYPE_IDX_LEN),
            (ctx::SPLIT_CODING_UNIT_FLAG, ctx::SPLIT_CODING_UNIT_FLAG_LEN),
            (
                ctx::CU_TRANSQUANT_BYPASS_FLAG,
                ctx::CU_TRANSQUANT_BYPASS_FLAG_LEN,
            ),
            (ctx::SKIP_FLAG, ctx::SKIP_FLAG_LEN),
            (ctx::CU_QP_DELTA, ctx::CU_QP_DELTA_LEN),
            (ctx::PRED_MODE_FLAG, ctx::PRED_MODE_FLAG_LEN),
            (ctx::PART_MODE, ctx::PART_MODE_LEN),
            (
                ctx::PREV_INTRA_LUMA_PRED_FLAG,
                ctx::PREV_INTRA_LUMA_PRED_FLAG_LEN,
            ),
            (ctx::INTRA_CHROMA_PRED_MODE, ctx::INTRA_CHROMA_PRED_MODE_LEN),
            (ctx::MERGE_FLAG, ctx::MERGE_FLAG_LEN),
            (ctx::MERGE_IDX, ctx::MERGE_IDX_LEN),
            (ctx::INTER_PRED_IDC, ctx::INTER_PRED_IDC_LEN),
            (ctx::REF_IDX_L0, ctx::REF_IDX_L0_LEN),
            (ctx::REF_IDX_L1, ctx::REF_IDX_L1_LEN),
            (ctx::ABS_MVD_GREATER0_FLAG, ctx::ABS_MVD_GREATER0_FLAG_LEN),
            (ctx::ABS_MVD_GREATER1_FLAG, ctx::ABS_MVD_GREATER1_FLAG_LEN),
            (ctx::MVP_LX_FLAG, ctx::MVP_LX_FLAG_LEN),
            (ctx::NO_RESIDUAL_DATA_FLAG, ctx::NO_RESIDUAL_DATA_FLAG_LEN),
            (ctx::SPLIT_TRANSFORM_FLAG, ctx::SPLIT_TRANSFORM_FLAG_LEN),
            (ctx::CBF_LUMA, ctx::CBF_LUMA_LEN),
            (ctx::CBF_CB_CR, ctx::CBF_CB_CR_LEN),
            (ctx::TRANSFORM_SKIP_FLAG, ctx::TRANSFORM_SKIP_FLAG_LEN),
            (ctx::EXPLICIT_RDPCM_FLAG, ctx::EXPLICIT_RDPCM_FLAG_LEN),
            (
                ctx::EXPLICIT_RDPCM_DIR_FLAG,
                ctx::EXPLICIT_RDPCM_DIR_FLAG_LEN,
            ),
            (
                ctx::LAST_SIGNIFICANT_COEFF_X_PREFIX,
                ctx::LAST_SIGNIFICANT_COEFF_X_PREFIX_LEN,
            ),
            (
                ctx::LAST_SIGNIFICANT_COEFF_Y_PREFIX,
                ctx::LAST_SIGNIFICANT_COEFF_Y_PREFIX_LEN,
            ),
            (
                ctx::SIGNIFICANT_COEFF_GROUP_FLAG,
                ctx::SIGNIFICANT_COEFF_GROUP_FLAG_LEN,
            ),
            (ctx::SIGNIFICANT_COEFF_FLAG, ctx::SIGNIFICANT_COEFF_FLAG_LEN),
            (
                ctx::COEFF_ABS_LEVEL_GREATER1_FLAG,
                ctx::COEFF_ABS_LEVEL_GREATER1_FLAG_LEN,
            ),
            (
                ctx::COEFF_ABS_LEVEL_GREATER2_FLAG,
                ctx::COEFF_ABS_LEVEL_GREATER2_FLAG_LEN,
            ),
            (ctx::LOG2_RES_SCALE_ABS, ctx::LOG2_RES_SCALE_ABS_LEN),
            (ctx::RES_SCALE_SIGN_FLAG, ctx::RES_SCALE_SIGN_FLAG_LEN),
            (
                ctx::CU_CHROMA_QP_OFFSET_FLAG,
                ctx::CU_CHROMA_QP_OFFSET_FLAG_LEN,
            ),
            (
                ctx::CU_CHROMA_QP_OFFSET_IDX,
                ctx::CU_CHROMA_QP_OFFSET_IDX_LEN,
            ),
        ];
        let mut expected = 0usize;
        for &(offset, len) in elements {
            assert_eq!(offset, expected, "non-contiguous offsets at {offset}");
            expected += len;
        }
        assert_eq!(expected, HEVC_CONTEXTS);
    }

    /// Spot check: known I-slice initial values for the first `split_cu_flag`
    /// context (init_value 139). This protects against transcription errors
    /// in the table itself.
    #[test]
    fn test_split_cu_flag_init_value() {
        assert_eq!(INIT_VALUES[0][ctx::SPLIT_CODING_UNIT_FLAG], 139);
        assert_eq!(INIT_VALUES[0][ctx::SPLIT_CODING_UNIT_FLAG + 1], 141);
        assert_eq!(INIT_VALUES[0][ctx::SPLIT_CODING_UNIT_FLAG + 2], 157);
    }

    /// Spot check: `last_significant_coeff_x_prefix[17] = 63` (last entry
    /// of an 18-element block) for I-slice. Catches off-by-ones at the
    /// 53..71 region boundary.
    #[test]
    fn test_last_sig_coeff_x_prefix_tail() {
        assert_eq!(
            INIT_VALUES[0][ctx::LAST_SIGNIFICANT_COEFF_X_PREFIX
                + ctx::LAST_SIGNIFICANT_COEFF_X_PREFIX_LEN
                - 1],
            63
        );
    }

    /// Spot check: the cbf_luma section is 2 contexts at offset 40 with
    /// values [111, 141] for I-slice.
    #[test]
    fn test_cbf_luma_init_values() {
        assert_eq!(INIT_VALUES[0][ctx::CBF_LUMA], 111);
        assert_eq!(INIT_VALUES[0][ctx::CBF_LUMA + 1], 141);
    }
}
