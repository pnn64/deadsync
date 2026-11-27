//! HEVC slice data parsing — coding tree, coding unit, intra/inter mode signaling.
//!
//! Phase 2c-1 scope: recursive `coding_quadtree` / `coding_unit` decoding for
//! the I-slice intra path. Phase 3d-3 extends to inter CUs in P/B slices:
//! `cu_skip_flag`, `pred_mode_flag`, merge/AMVP syntax, `mvd_coding`, and
//! `rqt_root_cbf`. Phase 3d-6 adds motion compensation: decoded MVs in
//! `tab_mvf` are used to interpolate prediction samples from reference frames
//! via `inter_pred::motion_compensation_cu`.
//!
//! The recursion structure mirrors FFmpeg `libavcodec/hevc/hevcdec.c`
//! (`hls_coding_quadtree`, `hls_coding_unit`, `hls_prediction_unit`,
//! `luma_intra_pred_mode`) so that decoded values match byte-for-byte.

use std::sync::Arc;

use crate::cabac::{CabacContexts, CabacReader};
use crate::cabac_tables::ctx;
use crate::dpb::DecodedPicture;
use crate::error::DecodeError;
use crate::inter_pred;
use crate::intra_pred::{
    ReferenceAvailability, add_residual, build_reference_samples, filter_reference_samples,
    predict_angular, predict_dc, predict_planar,
};
use crate::inverse_transform::apply_inverse_transform;
use crate::pixel::Pixel;
use crate::pps::Pps;
use crate::residual_coding::{ResidualBlock, ResidualPlane, ScanOrder, decode_residual_coding};
use crate::shared::SharedTable;
use crate::slice::SliceType;
use crate::sps::Sps;

/// HEVC luma intra prediction mode constants (spec table 8-1).
pub const INTRA_PLANAR: u8 = 0;
pub const INTRA_DC: u8 = 1;
pub const INTRA_ANGULAR_10: u8 = 10;
pub const INTRA_ANGULAR_26: u8 = 26;
/// Used by the chroma DM-substitution rule when the mapped chroma mode
/// equals the luma mode (spec table 8-3).
pub const INTRA_ANGULAR_34: u8 = 34;

/// HEVC prediction mode (spec table 7-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredMode {
    Intra,
    Inter,
    Skip,
}

/// HEVC partition mode (spec table 7-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartMode {
    Part2Nx2N,
    PartNxN,
    Part2NxN,
    PartNx2N,
    Part2NxnU,
    Part2NxnD,
    PartnLx2N,
    PartnRx2N,
}

/// Inter prediction direction (spec table 7-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterPredIdc {
    PredL0,
    PredL1,
    PredBi,
}

/// Motion vector (quarter-pel precision).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mv {
    pub x: i16,
    pub y: i16,
}

/// Per min-PU motion field entry, analogous to FFmpeg's `MvField`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MvField {
    pub mv: [Mv; 2],
    pub ref_idx: [i8; 2],
    /// Bit 0 = L0 active, bit 1 = L1 active.
    pub pred_flag: u8,
}

impl MvField {
    /// Spec-compliant merge candidate comparison (FFmpeg `compare_mv_ref_idx`).
    /// Only compares the active reference list components based on `pred_flag`.
    /// Inactive list fields (mv[1]/ref_idx[1] for L0-only, etc.) are ignored.
    fn merge_eq(&self, other: &Self) -> bool {
        if self.pred_flag != other.pred_flag {
            return false;
        }
        match self.pred_flag {
            3 => {
                // Bi-prediction: compare both lists.
                self.ref_idx[0] == other.ref_idx[0]
                    && self.mv[0] == other.mv[0]
                    && self.ref_idx[1] == other.ref_idx[1]
                    && self.mv[1] == other.mv[1]
            }
            1 => {
                // L0 only.
                self.ref_idx[0] == other.ref_idx[0] && self.mv[0] == other.mv[0]
            }
            2 => {
                // L1 only.
                self.ref_idx[1] == other.ref_idx[1] && self.mv[1] == other.mv[1]
            }
            _ => false,
        }
    }
}

/// Slice-level parameters needed by the CU tree for inter decoding.
/// Threaded through `decode_coding_quadtree` / `decode_coding_unit` so the
/// syntax decoders can see `slice_type`, `max_num_merge_cand`, etc.
#[derive(Debug, Clone)]
pub struct SliceParams {
    pub slice_type: SliceType,
    pub max_num_merge_cand: u32,
    pub num_ref_idx_l0_active: u32,
    pub num_ref_idx_l1_active: u32,
    pub mvd_l1_zero_flag: bool,
    /// `log2_parallel_merge_level_minus2 + 2` from PPS.
    /// Used to suppress self-referencing merge candidates (spec 8.5.3.2.2).
    pub log2_parallel_merge_level: u8,
    /// POC of the current picture being decoded.
    pub poc: i32,
    /// POC values of RefPicList0 entries (one per active L0 reference).
    /// `ref_pic_list_pocs[0][i]` is the POC of the picture at L0 index `i`.
    pub ref_pic_list_pocs: [Vec<i32>; 2],
    /// Whether each reference list entry is a long-term reference picture.
    pub ref_pic_list_is_lt: [Vec<bool>; 2],
    /// Reference lists of every slice of the current picture decoded so
    /// far, keyed by slice address (SliceAddrRs). Deblocking compares the
    /// reference pictures of a block in an earlier slice through that
    /// slice's lists, not the current slice's.
    pub picture_slice_ref_lists: Vec<(i32, crate::dpb::SliceRefLists)>,
    /// Phase 3d-6: actual reference frame pixel data for L0 MC.
    pub ref_frames_l0: Vec<Arc<DecodedPicture>>,
    /// Phase 3d-6: actual reference frame pixel data for L1 MC.
    pub ref_frames_l1: Vec<Arc<DecodedPicture>>,
    /// Phase 3e: collocated reference picture for temporal MVP.
    pub collocated_ref: Option<Arc<DecodedPicture>>,
    /// Phase 3e: `slice_temporal_mvp_enabled_flag` from the slice header.
    pub slice_temporal_mvp_enabled_flag: bool,
    /// Phase 3e: `collocated_from_l0_flag` from the slice header.
    /// true = collocated picture is from L0 (collocated_list = 0),
    /// false = from L1 (collocated_list = 1).
    pub collocated_from_l0_flag: bool,
    /// slice_deblocking_filter_disabled_flag: no boundary strengths (and so
    /// no deblocking) for CUs of this slice.
    pub deblocking_disabled: bool,
    /// Slice-level chroma QP offsets (summed with PPS offsets in chroma
    /// QP derivation per spec 8.6.1).
    pub slice_cb_qp_offset: i32,
    pub slice_cr_qp_offset: i32,
    /// Whether per-CU chroma QP offset signaling is enabled for this slice.
    pub cu_chroma_qp_offset_enabled_flag: bool,
    /// Whether weighted prediction is active for this slice.
    pub weighted_pred_flag: bool,
    /// Prediction weight table (only meaningful when `weighted_pred_flag`).
    pub pred_weight_table: crate::slice::PredWeightTable,
}

/// Per-picture mutable state needed during slice decode.
///
/// `tab_ct_depth` is per-min-CB and used for `split_cu_flag` neighbor
/// context derivation. `tab_ipm` is per-min-PU (4×4 in HEVC base profile)
/// and stores the decoded luma intra prediction mode for downstream use.
/// `y_plane`/`u_plane`/`v_plane` are the reconstructed picture planes that
/// `decode_transform_unit` writes prediction + residual into.
pub struct PictureState<P: Pixel> {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    /// Chroma sample bit depth (BitDepthC); may differ from luma.
    pub bit_depth_c: u8,
    pub log2_min_cb_size: u8,
    pub log2_min_pu_size: u8,
    pub log2_ctb_size: u8,
    /// PicWidthInCtbsY, for raster CTB addresses without a division.
    pub width_in_ctbs: usize,
    pub min_cb_width: usize,
    pub min_pu_width: usize,
    pub tab_ct_depth: SharedTable<u8>,
    pub tab_ipm: SharedTable<u8>,
    /// Per-min-PU chroma intra mode (luma-mode space). For 4:2:0/4:2:2 the
    /// whole CU shares one chroma mode; for 4:4:4 with PART_NxN each of the
    /// four PUs has its own, so the transform unit looks the mode up by
    /// position here rather than from `last_chroma_pred_mode`.
    pub tab_chroma_ipm: SharedTable<u8>,
    pub y_plane: SharedTable<P>,
    pub u_plane: SharedTable<P>,
    pub v_plane: SharedTable<P>,
    pub y_stride: usize,
    pub uv_stride: usize,
    /// `ChromaArrayType` (1 = 4:2:0, 2 = 4:2:2, 3 = 4:4:4). Selects the chroma
    /// geometry used across prediction, residual, and loop filters.
    pub chroma_array_type: u32,
    /// `log2(SubWidthC)` / `log2(SubHeightC)`: 1 for a subsampled chroma
    /// dimension (4:2:0 both; 4:2:2 width only), 0 for a full-resolution one
    /// (4:4:4 both; 4:2:2 height). Used to map luma coordinates to chroma.
    pub chroma_shift_w: u32,
    pub chroma_shift_h: u32,

    /// Phase 2c-1 sentinel: most recently decoded luma intra mode of the
    /// most recently decoded CU. Will go away once full CU decode is wired up.
    pub last_luma_pred_mode: u8,
    /// Phase 2c-1 sentinel: most recently decoded chroma intra mode (mapped
    /// to the luma mode space, not the raw `chroma_mode_idx`).
    pub last_chroma_pred_mode: u8,
    /// Number of CUs visited during decode (sentinel for tests).
    pub cu_count: u32,

    /// Phase 2c-2 sentinels — most recent values from `transform_tree`.
    pub last_split_transform_flag: bool,
    pub last_cbf_luma: bool,
    pub last_cbf_cb: bool,
    pub last_cbf_cr: bool,
    /// Signed CU QP delta as decoded for the most recent TU. 0 if not coded.
    pub last_cu_qp_delta: i32,
    /// Reset at the top of each QP group in decode_coding_quadtree;
    /// set to true once the first TU with non-zero CBF decodes cu_qp_delta.
    pub is_cu_qp_delta_coded: bool,
    /// Whether `cu_chroma_qp_offset_flag` has already been decoded for the
    /// current CU. Reset at the start of each CU (spec 7.3.8.11).
    pub is_cu_chroma_qp_offset_coded: bool,
    /// Per-CU Cb chroma QP offset selected from PPS offset list (or 0).
    pub cu_qp_offset_cb: i32,
    /// Per-CU Cr chroma QP offset selected from PPS offset list (or 0).
    pub cu_qp_offset_cr: i32,
    /// Effective QP after applying `last_cu_qp_delta` to the predicted QP
    /// (spec 8.6.1). Equals `slice_qp_y` while `cu_qp_delta_enabled_flag=0`.
    pub last_qp_y: i32,
    /// Predicted QP (`qPy_pred`) inherited across QP groups (spec 8.6.1 /
    /// FFmpeg `HEVCLocalContext::qPy_pred`). Updated at the end of each
    /// QP-group-aligned CU / split node; consulted when the left/above
    /// neighbor used by `get_qPy_pred` is unavailable.
    pub qpy_pred: i32,
    /// Set at slice start (for non-dependent segments) and at WPP/tile
    /// row starts. Forces `get_qPy_pred` to fall back to `slice_qp` for
    /// the first QP group of the reset region (spec 8.6.1).
    pub first_qp_group: bool,
    /// Origin of the coding unit being decoded. cu_qp_delta inside its
    /// transform tree derives the QP prediction from the CU origin (spec
    /// 8.6.1 xCb / yCb), not from the transform block.
    pub cu_origin: (u32, u32),
    /// The coding unit being decoded is a single prediction unit (skip or
    /// PART_2Nx2N): no PU edges inside it for deblocking.
    pub cu_single_pu: bool,
    /// Luma residual of the current transform unit after the inverse
    /// transform, kept for cross-component prediction (spec 8.6.6).
    pub ccp_luma_residual: Vec<i16>,
    /// Coefficient / residual buffer lent to each transform block in turn,
    /// so residual decoding does not allocate per block.
    pub coeff_buf: Vec<i16>,
    /// Deblocked Y / Cb / Cr copy that SAO reads from (reused per picture).
    pub sao_snapshot: [Vec<P>; 3],

    /// Phase 2c-3 sentinel — most recently decoded luma residual block.
    pub last_luma_residual: Option<ResidualBlock>,

    /// Phase 3b-1 deblocking: per-min-CB QP for deblock filter strength.
    /// Luma QP per min-CB. Signed: QpY ranges over -QpBdOffsetY..=51.
    pub tab_qp_y: SharedTable<i8>,
    /// Phase 3b-1 deblocking: per-4×4 boundary strength for vertical edges.
    /// Indexed by `(y/4) * (width/4) + (x/4)` (matches FFmpeg's `vertical_bs`).
    pub bs_vertical: SharedTable<u8>,
    /// Phase 3b-1 deblocking: per-4×4 boundary strength for horizontal edges.
    pub bs_horizontal: SharedTable<u8>,
    /// Phase 3b-1 deblocking: per min-TB cbf_luma flag. Indexed by
    /// `(y >> log2_min_tb_size) * min_tb_width + (x >> log2_min_tb_size)`.
    pub tab_cbf_luma: SharedTable<u8>,
    /// log2 of minimum transform block size (for deblocking cbf_luma table).
    pub log2_min_tb_size: u32,
    /// Phase 3b-2 SAO: per-CTB SAO parameters, indexed by CTB raster address.
    pub sao_params: SharedTable<crate::sao::SaoParams>,
    /// Phase 3c-1 multi-slice: per-CTB slice address (`slice_segment_address`
    /// of the slice the CTB belongs to), indexed by CTB raster address.
    /// `-1` means the CTB has not been decoded yet (not part of any slice).
    pub tab_slice_addr_rs: SharedTable<i32>,
    /// Per-CTB `slice_loop_filter_across_slices_enabled_flag`. When false for
    /// either side of a CTB boundary, deblocking/SAO across that slice boundary
    /// is suppressed. Indexed by CTB raster address.
    pub filter_slice_edges: SharedTable<bool>,
    /// Phase 3c-2 tiles: per-CTB tile id (0-based), indexed by CTB raster
    /// address. For `tiles_enabled_flag = 0` pictures this is all zeros.
    /// Consulted by `compute_luma_avail` to treat cross-tile neighbor
    /// samples as unavailable for intra prediction (spec 6.4.4 / 8.4.2).
    pub tab_tile_id: SharedTable<u32>,
    /// Per CTB (raster index): its tile-scan address, i.e. its position in
    /// decoding order. SAO uses it to tell which of two slices is later.
    pub tab_ctb_ts: SharedTable<u32>,
    /// Per CTB: the deblocking (beta_offset, tc_offset) of the slice that
    /// contains it, i.e. slice_beta_offset_div2 * 2 and slice_tc_offset_div2
    /// * 2. Edges use the values of the slice containing their Q side.
    pub tab_deblock_offsets: SharedTable<(i8, i8)>,
    /// Per 4x4 luma block: 1 where deblocking and SAO must leave samples
    /// unchanged (a cu_transquant_bypass CU, or a PCM CU when
    /// pcm_loop_filter_disabled_flag is set; spec 8.7.2.5.7, 8.7.3).
    pub tab_no_filter: SharedTable<u8>,
    /// True once any block of this picture is marked in `tab_no_filter`, so
    /// the loop filters can skip the lookups for ordinary pictures.
    pub any_no_filter: bool,
    /// PPS `loop_filter_across_tiles_enabled_flag`: when false, deblocking
    /// and SAO must not use samples across a tile boundary.
    pub filter_across_tiles: bool,

    /// Phase 3d-3: per min-PU motion field (MV + ref_idx + pred_flag).
    /// Indexed by `(y >> log2_min_pu_size) * min_pu_width + (x >> log2_min_pu_size)`.
    pub tab_mvf: SharedTable<MvField>,
    /// Phase 3d-3: per min-CB skip flag, used for `cu_skip_flag` neighbor
    /// context derivation. Indexed the same as `tab_ct_depth`.
    pub tab_skip_flag: SharedTable<u8>,
    /// Z-scan order address table for intra prediction availability.
    /// `min_tb_addr_zs[y * min_tb_addr_zs_stride + x]` gives the Z-scan order
    /// index for the min-TB at position (x, y) in min-TB units within the
    /// picture. Used to determine whether bottom-left / upper-right reference
    /// samples are available (decoded before the current block in Z-scan order).
    /// Matches FFmpeg's `pps->min_tb_addr_zs`.
    /// Shared between pictures: it depends only on the SPS geometry, so the
    /// decoder builds it once (see `build_min_tb_addr_zs`).
    pub min_tb_addr_zs: std::sync::Arc<Vec<i32>>,
    /// Stride (width in min-TB units + 1 for the -1 sentinel column) of the
    /// Z-scan table.
    pub min_tb_addr_zs_stride: usize,
    /// tb_mask = (1 << (ctb_log2 - min_tb_log2)) * pic_width_in_ctbs - 1
    /// Actually, the number of min-TBs across the picture width.
    pub min_tb_width: usize,
    /// Motion compensation scratch buffers, reused by every inter block of
    /// this picture (see `inter_pred::McScratch`).
    pub mc_scratch: crate::inter_pred::McScratch<P>,
    /// Frame threading: the loop filters run row by row as this picture
    /// decodes (`crate::rowfilter`); `None` otherwise.
    pub row_filter: Option<Box<crate::rowfilter::RowFilter<P>>>,
    /// Which CTBs are decoded (in every mode; see `rowfilter::DecodedRows`).
    pub decoded_rows: crate::rowfilter::DecodedRows,
    /// A wavefront worker's state: blocks are written to the planes one
    /// row at a time (see `with_block`).
    pub row_exact: bool,
    /// Scratch block for `with_block`.
    pub block_buf: Vec<P>,
}

/// Buffers of earlier pictures that `PictureState::new_recycled` reuses.
pub struct RecycledBuffers<P: Pixel> {
    /// A finished picture's state: its tables and scratch buffers (planes,
    /// motion field and slice map were moved into the decoded picture).
    pub state: Option<PictureState<P>>,
    /// Luma, Cb and Cr planes of a picture that left the DPB.
    pub planes: Option<[Vec<P>; 3]>,
    /// Motion field of a picture that left the DPB.
    pub mvf: Option<Vec<MvField>>,
}

impl<P: Pixel> Default for RecycledBuffers<P> {
    fn default() -> Self {
        Self {
            state: None,
            planes: None,
            mvf: None,
        }
    }
}

// ---- Wavefront workers -------------------------------------------------------

impl<P: Pixel> PictureState<P> {
    /// A picture state for a worker thread decoding CTB rows of this
    /// picture in parallel (wavefront decoding, see `crate::wavefront`).
    /// Its tables and planes are views of this state's buffers
    /// (`SharedTable::view`); its per-thread decoding state starts as a
    /// copy of this state's, and it has its own scratch buffers.
    ///
    /// # Safety
    ///
    /// As for `SharedTable::view`: every worker state must be dropped
    /// before `self` is used again, and the workers must follow the
    /// wavefront protocol (`crate::shared`).
    pub(crate) unsafe fn worker_view(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            bit_depth: self.bit_depth,
            bit_depth_c: self.bit_depth_c,
            log2_min_cb_size: self.log2_min_cb_size,
            log2_min_pu_size: self.log2_min_pu_size,
            log2_ctb_size: self.log2_ctb_size,
            width_in_ctbs: self.width_in_ctbs,
            min_cb_width: self.min_cb_width,
            min_pu_width: self.min_pu_width,
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_ct_depth: unsafe { self.tab_ct_depth.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_ipm: unsafe { self.tab_ipm.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_chroma_ipm: unsafe { self.tab_chroma_ipm.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            y_plane: unsafe { self.y_plane.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            u_plane: unsafe { self.u_plane.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            v_plane: unsafe { self.v_plane.view() },
            y_stride: self.y_stride,
            uv_stride: self.uv_stride,
            chroma_array_type: self.chroma_array_type,
            chroma_shift_w: self.chroma_shift_w,
            chroma_shift_h: self.chroma_shift_h,
            last_luma_pred_mode: self.last_luma_pred_mode,
            last_chroma_pred_mode: self.last_chroma_pred_mode,
            cu_count: self.cu_count,
            last_split_transform_flag: self.last_split_transform_flag,
            last_cbf_luma: self.last_cbf_luma,
            last_cbf_cb: self.last_cbf_cb,
            last_cbf_cr: self.last_cbf_cr,
            last_cu_qp_delta: self.last_cu_qp_delta,
            is_cu_qp_delta_coded: self.is_cu_qp_delta_coded,
            is_cu_chroma_qp_offset_coded: self.is_cu_chroma_qp_offset_coded,
            cu_qp_offset_cb: self.cu_qp_offset_cb,
            cu_qp_offset_cr: self.cu_qp_offset_cr,
            last_qp_y: self.last_qp_y,
            qpy_pred: self.qpy_pred,
            first_qp_group: self.first_qp_group,
            cu_origin: self.cu_origin,
            cu_single_pu: self.cu_single_pu,
            ccp_luma_residual: Vec::new(),
            coeff_buf: Vec::new(),
            sao_snapshot: Default::default(),
            last_luma_residual: None,
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_qp_y: unsafe { self.tab_qp_y.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            bs_vertical: unsafe { self.bs_vertical.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            bs_horizontal: unsafe { self.bs_horizontal.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_cbf_luma: unsafe { self.tab_cbf_luma.view() },
            log2_min_tb_size: self.log2_min_tb_size,
            // SAFETY: forwarded to the caller (see the function's contract).
            sao_params: unsafe { self.sao_params.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_slice_addr_rs: unsafe { self.tab_slice_addr_rs.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            filter_slice_edges: unsafe { self.filter_slice_edges.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_tile_id: unsafe { self.tab_tile_id.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_ctb_ts: unsafe { self.tab_ctb_ts.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_deblock_offsets: unsafe { self.tab_deblock_offsets.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_no_filter: unsafe { self.tab_no_filter.view() },
            any_no_filter: self.any_no_filter,
            filter_across_tiles: self.filter_across_tiles,
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_mvf: unsafe { self.tab_mvf.view() },
            // SAFETY: forwarded to the caller (see the function's contract).
            tab_skip_flag: unsafe { self.tab_skip_flag.view() },
            min_tb_addr_zs: std::sync::Arc::clone(&self.min_tb_addr_zs),
            min_tb_addr_zs_stride: self.min_tb_addr_zs_stride,
            min_tb_width: self.min_tb_width,
            mc_scratch: crate::inter_pred::McScratch::new(),
            row_filter: None,
            decoded_rows: Default::default(),
            row_exact: true,
            block_buf: Vec::new(),
        }
    }

    /// Hand this state's planes, motion field and slice map to a picture
    /// that later pictures reference while this state decodes it (frame
    /// threading), keeping views of them for the decode.
    ///
    /// # Safety
    ///
    /// The returned tables must stay alive (and not be accessed mutably)
    /// until `release_lent` drops the views, and other threads may read
    /// them only where the picture's progress says they are final.
    pub(crate) unsafe fn lend_shared(
        &mut self,
    ) -> ([SharedTable<P>; 3], SharedTable<MvField>, SharedTable<i32>) {
        fn lend<T>(t: &mut SharedTable<T>) -> SharedTable<T> {
            let owner = std::mem::take(t);
            // SAFETY: forwarded to the caller of `lend_shared`.
            *t = unsafe { owner.view() };
            owner
        }
        (
            [
                lend(&mut self.y_plane),
                lend(&mut self.u_plane),
                lend(&mut self.v_plane),
            ],
            lend(&mut self.tab_mvf),
            lend(&mut self.tab_slice_addr_rs),
        )
    }

    /// Drop the views `lend_shared` left (keeping the other tables, which
    /// stay owned and are recycled with the state).
    pub(crate) fn release_lent(&mut self) {
        self.y_plane = SharedTable::default();
        self.u_plane = SharedTable::default();
        self.v_plane = SharedTable::default();
        self.tab_mvf = SharedTable::default();
        self.tab_slice_addr_rs = SharedTable::default();
    }

    /// Drop a worker state's table views (replacing them with empty owned
    /// tables), keeping its per-thread decoding state. Every worker calls
    /// this before the picture state it views is used again.
    pub(crate) fn release_views(&mut self) {
        self.tab_ct_depth = SharedTable::default();
        self.tab_ipm = SharedTable::default();
        self.tab_chroma_ipm = SharedTable::default();
        self.y_plane = SharedTable::default();
        self.u_plane = SharedTable::default();
        self.v_plane = SharedTable::default();
        self.tab_qp_y = SharedTable::default();
        self.bs_vertical = SharedTable::default();
        self.bs_horizontal = SharedTable::default();
        self.tab_cbf_luma = SharedTable::default();
        self.sao_params = SharedTable::default();
        self.tab_slice_addr_rs = SharedTable::default();
        self.filter_slice_edges = SharedTable::default();
        self.tab_tile_id = SharedTable::default();
        self.tab_ctb_ts = SharedTable::default();
        self.tab_deblock_offsets = SharedTable::default();
        self.tab_no_filter = SharedTable::default();
        self.tab_mvf = SharedTable::default();
        self.tab_skip_flag = SharedTable::default();
    }

    /// After the workers of one slice finished: take the per-thread
    /// decoding state of `last`, the worker that decoded the slice's last
    /// CTB (as if one thread had decoded the whole slice), and merge the
    /// picture-level values every worker may have set.
    pub(crate) fn take_worker_state(&mut self, last: &Self, all: &[Self]) {
        self.last_luma_pred_mode = last.last_luma_pred_mode;
        self.last_chroma_pred_mode = last.last_chroma_pred_mode;
        self.last_split_transform_flag = last.last_split_transform_flag;
        self.last_cbf_luma = last.last_cbf_luma;
        self.last_cbf_cb = last.last_cbf_cb;
        self.last_cbf_cr = last.last_cbf_cr;
        self.last_cu_qp_delta = last.last_cu_qp_delta;
        self.is_cu_qp_delta_coded = last.is_cu_qp_delta_coded;
        self.is_cu_chroma_qp_offset_coded = last.is_cu_chroma_qp_offset_coded;
        self.cu_qp_offset_cb = last.cu_qp_offset_cb;
        self.cu_qp_offset_cr = last.cu_qp_offset_cr;
        self.last_qp_y = last.last_qp_y;
        self.qpy_pred = last.qpy_pred;
        self.first_qp_group = last.first_qp_group;
        self.cu_origin = last.cu_origin;
        self.cu_single_pu = last.cu_single_pu;
        for w in all {
            self.any_no_filter |= w.any_no_filter;
            self.cu_count += w.cu_count;
        }
    }

    /// Where each table's elements lie: `(row_len, table rows per CTB row,
    /// table columns per CTB column)` (`crate::shared::Geometry`), for the
    /// picture-level tables a wavefront worker writes.
    pub(crate) fn table_geometries(&self) -> TableGeometries {
        use crate::shared::Geometry;
        let ctb = 1usize << self.log2_ctb_size;
        let grid = |row_len: usize, log2_unit: u32| Geometry {
            row_len,
            ctb_rows: ctb >> log2_unit,
            ctb_cols: ctb >> log2_unit,
        };
        TableGeometries {
            luma: grid(self.y_stride, 0),
            chroma: Geometry {
                row_len: self.uv_stride,
                ctb_rows: ctb >> self.chroma_shift_h,
                ctb_cols: ctb >> self.chroma_shift_w,
            },
            min_cb: grid(self.min_cb_width, self.log2_min_cb_size as u32),
            min_pu: grid(self.min_pu_width, self.log2_min_pu_size as u32),
            min_tb: grid(self.min_tb_width, self.log2_min_tb_size),
            // Boundary strengths are indexed with the unaligned width.
            grid4: grid(self.width as usize >> 2, 2),
            ctb: grid(self.width_in_ctbs, self.log2_ctb_size as u32),
        }
    }

    /// Attach wavefront access checking to every table view (test builds).
    #[cfg(test)]
    pub(crate) fn attach_checks(&mut self, check: &std::sync::Arc<crate::shared::AccessCheck>) {
        let g = self.table_geometries();
        let c = || std::sync::Arc::clone(check);
        self.y_plane.set_check(c(), g.luma, "y_plane");
        self.u_plane.set_check(c(), g.chroma, "u_plane");
        self.v_plane.set_check(c(), g.chroma, "v_plane");
        self.tab_ct_depth.set_check(c(), g.min_cb, "tab_ct_depth");
        self.tab_qp_y.set_check(c(), g.min_cb, "tab_qp_y");
        self.tab_skip_flag.set_check(c(), g.min_cb, "tab_skip_flag");
        self.tab_ipm.set_check(c(), g.min_pu, "tab_ipm");
        self.tab_chroma_ipm
            .set_check(c(), g.min_pu, "tab_chroma_ipm");
        self.tab_no_filter.set_check(c(), g.min_pu, "tab_no_filter");
        self.tab_mvf.set_check(c(), g.min_pu, "tab_mvf");
        self.tab_cbf_luma.set_check(c(), g.min_tb, "tab_cbf_luma");
        self.bs_vertical.set_check(c(), g.grid4, "bs_vertical");
        self.bs_horizontal.set_check(c(), g.grid4, "bs_horizontal");
        self.sao_params.set_check(c(), g.ctb, "sao_params");
        self.tab_slice_addr_rs
            .set_check(c(), g.ctb, "tab_slice_addr_rs");
        self.filter_slice_edges
            .set_check(c(), g.ctb, "filter_slice_edges");
        self.tab_tile_id.set_check(c(), g.ctb, "tab_tile_id");
        self.tab_ctb_ts.set_check(c(), g.ctb, "tab_ctb_ts");
        self.tab_deblock_offsets
            .set_check(c(), g.ctb, "tab_deblock_offsets");
    }

    /// Return the tables of CTBs `cols` of CTB row `row` to their initial
    /// values (those of `new_recycled`), as if the CTBs had never been
    /// decoded. Used for rows a wavefront decode ran ahead into but the
    /// slice did not reach (an error or an early end above them). The
    /// samples are left alone: `fill_undecoded_ctbs` greys out CTBs whose
    /// slice address is -1.
    pub(crate) fn reset_ctbs(&mut self, row: usize, cols: std::ops::Range<usize>) {
        fn reset<T: Clone>(
            t: &mut SharedTable<T>,
            g: crate::shared::Geometry,
            row: usize,
            cols: &std::ops::Range<usize>,
            value: T,
        ) {
            let (c0, c1) = (
                cols.start * g.ctb_cols,
                (cols.end * g.ctb_cols).min(g.row_len),
            );
            for r in row * g.ctb_rows..(row + 1) * g.ctb_rows {
                let (a, b) = (r * g.row_len + c0, r * g.row_len + c1);
                if b <= t.len() && a < b {
                    t[a..b].fill(value.clone());
                }
            }
        }
        let g = self.table_geometries();
        reset(&mut self.tab_ct_depth, g.min_cb, row, &cols, 0);
        reset(&mut self.tab_qp_y, g.min_cb, row, &cols, 0);
        reset(&mut self.tab_skip_flag, g.min_cb, row, &cols, 0);
        reset(&mut self.tab_ipm, g.min_pu, row, &cols, INTRA_DC);
        reset(&mut self.tab_chroma_ipm, g.min_pu, row, &cols, INTRA_DC);
        reset(&mut self.tab_no_filter, g.min_pu, row, &cols, 0);
        reset(&mut self.tab_mvf, g.min_pu, row, &cols, MvField::default());
        reset(&mut self.tab_cbf_luma, g.min_tb, row, &cols, 0);
        reset(&mut self.bs_vertical, g.grid4, row, &cols, 0);
        reset(&mut self.bs_horizontal, g.grid4, row, &cols, 0);
        reset(
            &mut self.sao_params,
            g.ctb,
            row,
            &cols,
            crate::sao::SaoParams::default(),
        );
        reset(&mut self.tab_slice_addr_rs, g.ctb, row, &cols, -1);
        reset(&mut self.filter_slice_edges, g.ctb, row, &cols, true);
        reset(&mut self.tab_ctb_ts, g.ctb, row, &cols, 0);
        reset(&mut self.tab_deblock_offsets, g.ctb, row, &cols, (0, 0));
    }
}

/// Table geometries of a picture state (see `PictureState::table_geometries`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TableGeometries {
    pub luma: crate::shared::Geometry,
    pub chroma: crate::shared::Geometry,
    pub min_cb: crate::shared::Geometry,
    pub min_pu: crate::shared::Geometry,
    pub min_tb: crate::shared::Geometry,
    pub grid4: crate::shared::Geometry,
    pub ctb: crate::shared::Geometry,
}

/// Run `f(dst, dst_stride)` on the `w` x `h` block of `plane` whose first
/// sample is `plane[off]` (row stride `stride`).
///
/// Normally `dst` is the plane range from the block's first to its last
/// sample. A wavefront worker (`row_exact`) must not do that: such a range
/// also covers the start of the block's last row, which can be the bottom
/// sample row of the CTB row, read at the same time by the row below (as
/// intra references). It gets a copy in `buf` instead, read and written
/// back one exact row at a time.
pub(crate) fn with_block<P: Pixel, R>(
    plane: &mut SharedTable<P>,
    buf: &mut Vec<P>,
    row_exact: bool,
    off: usize,
    stride: usize,
    (w, h): (usize, usize),
    f: impl FnOnce(&mut [P], usize) -> R,
) -> R {
    if h == 0 || w == 0 {
        return f(&mut [], stride);
    }
    if !row_exact {
        return f(&mut plane[off..off + (h - 1) * stride + w], stride);
    }
    buf.clear();
    for j in 0..h {
        buf.extend_from_slice(&plane[off + j * stride..off + j * stride + w]);
    }
    let r = f(&mut buf[..], w);
    for j in 0..h {
        plane[off + j * stride..off + j * stride + w].copy_from_slice(&buf[j * w..(j + 1) * w]);
    }
    r
}

/// `buf` (or a new buffer) holding `n` copies of `value`. Works on `Vec`s
/// and `SharedTable`s alike (in and out).
fn refill<T: Clone, R: From<Vec<T>>>(buf: Option<impl Into<Vec<T>>>, n: usize, value: T) -> R {
    let v = match buf {
        Some(v) => {
            let mut v: Vec<T> = v.into();
            v.clear();
            v.resize(n, value);
            v
        }
        None => vec![value; n],
    };
    v.into()
}

/// A sample plane of `n` samples: `buf` as it is when it has that size,
/// otherwise zero-filled.
fn plane_buf<P: Pixel, R: From<Vec<P>>>(buf: Option<impl Into<Vec<P>>>, n: usize) -> R {
    let v: Option<Vec<P>> = buf.map(Into::into);
    match v {
        Some(v) if v.len() == n => v.into(),
        other => refill(other, n, P::zero()),
    }
}

impl<P: Pixel> PictureState<P> {
    pub fn new(sps: &Sps) -> Self {
        Self::new_with_zscan(sps, std::sync::Arc::new(build_min_tb_addr_zs(sps)))
    }

    /// Like `new`, reusing a Z-scan table previously built by
    /// `build_min_tb_addr_zs` for an SPS with the same picture geometry.
    pub fn new_with_zscan(sps: &Sps, zscan: std::sync::Arc<Vec<i32>>) -> Self {
        Self::new_recycled(sps, zscan, RecycledBuffers::default())
    }

    /// Like `new_with_zscan`, reusing buffers of earlier pictures instead of
    /// allocating: the tables of a finished picture state (refilled with
    /// their initial values) and the sample planes / motion field of a
    /// picture that left the DPB. Planes are reused as they are, without
    /// clearing: decoding writes every sample inside the picture, and no
    /// process reads the CTU-alignment padding.
    pub fn new_recycled(
        sps: &Sps,
        zscan: std::sync::Arc<Vec<i32>>,
        recycled: RecycledBuffers<P>,
    ) -> Self {
        let mut old = recycled.state;
        // Take field `$f` of the recycled state, if any.
        macro_rules! reuse {
            ($f:ident) => {
                old.as_mut().map(|o| std::mem::take(&mut o.$f))
            };
        }
        let [y_old, u_old, v_old] = match recycled.planes {
            Some([y, u, v]) => [Some(y), Some(u), Some(v)],
            None => [None, None, None],
        };
        let log2_min_cb_size = sps.min_cb_log2_size_y;
        // HEVC base profile pins min PU size to 4×4 (spec 7.4.3.2.1).
        let log2_min_pu_size = 2u8;
        let log2_ctb_size = sps.ctb_log2_size_y;
        let w = sps.pic_width_in_luma_samples;
        let h = sps.pic_height_in_luma_samples;
        // Use CTU-aligned dimensions for per-min-CB and per-min-PU tables so
        // that CUs in the last CTU row/column (which may extend past the
        // picture boundary) don't write out of bounds.
        let ctb_size = 1u32 << log2_ctb_size;
        let w_aligned = w.div_ceil(ctb_size) * ctb_size;
        let h_aligned = h.div_ceil(ctb_size) * ctb_size;
        let min_cb_width = (w_aligned >> log2_min_cb_size) as usize;
        let min_cb_height = (h_aligned >> log2_min_cb_size) as usize;
        let min_pu_width = (w_aligned >> log2_min_pu_size) as usize;
        let min_pu_height = (h_aligned >> log2_min_pu_size) as usize;
        let log2_min_tb_size = sps.min_tb_log2_size_y as u32;
        let min_tb_width = (w_aligned >> log2_min_tb_size) as usize;
        let min_tb_height = (h_aligned >> log2_min_tb_size) as usize;
        // Pixel planes use CTU-aligned dimensions so that CUs at the picture
        // edge (which extend into the padding area) can read/write without OOB.
        // Chroma geometry (spec table 6-1). `chroma_shift_*` is `log2` of the
        // subsampling factor, so `luma >> shift` yields the chroma coordinate
        // and `w_aligned >> shift` the chroma plane extent. For 4:2:0 both are
        // 1, reproducing the previous `>> 1` / `/ 2` behaviour exactly.
        let chroma_array_type = sps.chroma_array_type();
        let chroma_shift_w = if sps.sub_width_c() == 2 { 1 } else { 0 };
        let chroma_shift_h = if sps.sub_height_c() == 2 { 1 } else { 0 };
        let y_stride = w_aligned as usize;
        let uv_stride = (w_aligned >> chroma_shift_w) as usize;
        Self {
            width: w,
            height: h,
            bit_depth: sps.bit_depth_luma,
            bit_depth_c: sps.bit_depth_chroma,
            log2_min_cb_size,
            log2_min_pu_size,
            log2_ctb_size,
            width_in_ctbs: (w as usize).div_ceil(1usize << log2_ctb_size),
            min_cb_width,
            min_pu_width,
            tab_ct_depth: refill(reuse!(tab_ct_depth), min_cb_width * min_cb_height, 0u8),
            // Default IPM is INTRA_DC (matches FFmpeg
            // `intra_prediction_unit_default_value`).
            tab_ipm: refill(reuse!(tab_ipm), min_pu_width * min_pu_height, INTRA_DC),
            tab_chroma_ipm: refill(
                reuse!(tab_chroma_ipm),
                min_pu_width * min_pu_height,
                INTRA_DC,
            ),
            y_plane: plane_buf(y_old, (w_aligned * h_aligned) as usize),
            u_plane: plane_buf(
                u_old,
                ((w_aligned >> chroma_shift_w) * (h_aligned >> chroma_shift_h)) as usize,
            ),
            v_plane: plane_buf(
                v_old,
                ((w_aligned >> chroma_shift_w) * (h_aligned >> chroma_shift_h)) as usize,
            ),
            y_stride,
            uv_stride,
            chroma_array_type,
            chroma_shift_w,
            chroma_shift_h,
            last_luma_pred_mode: 0,
            last_chroma_pred_mode: 0,
            cu_count: 0,
            last_split_transform_flag: false,
            last_cbf_luma: false,
            last_cbf_cb: false,
            last_cbf_cr: false,
            last_cu_qp_delta: 0,
            is_cu_qp_delta_coded: false,
            is_cu_chroma_qp_offset_coded: false,
            cu_qp_offset_cb: 0,
            cu_qp_offset_cr: 0,
            last_qp_y: 0,
            qpy_pred: 0,
            first_qp_group: false,
            cu_origin: (0, 0),
            cu_single_pu: true,
            ccp_luma_residual: reuse!(ccp_luma_residual).unwrap_or_default(),
            coeff_buf: reuse!(coeff_buf).unwrap_or_default(),
            sao_snapshot: reuse!(sao_snapshot).unwrap_or_default(),
            last_luma_residual: None,
            tab_qp_y: refill(reuse!(tab_qp_y), min_cb_width * min_cb_height, 0i8),
            bs_vertical: refill(
                reuse!(bs_vertical),
                ((w_aligned / 4) * (h_aligned / 4)) as usize,
                0u8,
            ),
            bs_horizontal: refill(
                reuse!(bs_horizontal),
                ((w_aligned / 4) * (h_aligned / 4)) as usize,
                0u8,
            ),
            tab_cbf_luma: refill(reuse!(tab_cbf_luma), min_tb_width * min_tb_height, 0u8),
            log2_min_tb_size,
            sao_params: {
                let ctb_size = 1u32 << log2_ctb_size;
                let pw = w.div_ceil(ctb_size) as usize;
                let ph = h.div_ceil(ctb_size) as usize;
                refill(
                    reuse!(sao_params),
                    pw * ph,
                    crate::sao::SaoParams::default(),
                )
            },
            tab_slice_addr_rs: {
                let ctb_size = 1u32 << log2_ctb_size;
                let pw = w.div_ceil(ctb_size) as usize;
                let ph = h.div_ceil(ctb_size) as usize;
                refill(reuse!(tab_slice_addr_rs), pw * ph, -1i32)
            },
            filter_slice_edges: {
                let ctb_size = 1u32 << log2_ctb_size;
                let pw = w.div_ceil(ctb_size) as usize;
                let ph = h.div_ceil(ctb_size) as usize;
                refill(reuse!(filter_slice_edges), pw * ph, true)
            },
            tab_tile_id: {
                let ctb_size = 1u32 << log2_ctb_size;
                let pw = w.div_ceil(ctb_size) as usize;
                let ph = h.div_ceil(ctb_size) as usize;
                refill(reuse!(tab_tile_id), pw * ph, 0u32)
            },
            filter_across_tiles: true,
            tab_ctb_ts: {
                let ctb_size = 1u32 << log2_ctb_size;
                refill(
                    reuse!(tab_ctb_ts),
                    (w.div_ceil(ctb_size) * h.div_ceil(ctb_size)) as usize,
                    0,
                )
            },
            tab_deblock_offsets: {
                let ctb_size = 1u32 << log2_ctb_size;
                refill(
                    reuse!(tab_deblock_offsets),
                    (w.div_ceil(ctb_size) * h.div_ceil(ctb_size)) as usize,
                    (0, 0),
                )
            },
            tab_no_filter: refill(reuse!(tab_no_filter), min_pu_width * min_pu_height, 0u8),
            any_no_filter: false,
            tab_mvf: refill(
                recycled.mvf.or_else(|| reuse!(tab_mvf).map(Vec::from)),
                min_pu_width * min_pu_height,
                MvField::default(),
            ),
            tab_skip_flag: refill(reuse!(tab_skip_flag), min_cb_width * min_cb_height, 0u8),
            min_tb_addr_zs: zscan,
            min_tb_addr_zs_stride: {
                let log2_diff = log2_ctb_size.saturating_sub(sps.min_tb_log2_size_y) as u32;
                let tb_per_ctb = 1u32 << log2_diff;
                let ctb_size = 1u32 << log2_ctb_size;
                let pic_w_in_ctbs = w.div_ceil(ctb_size);
                (pic_w_in_ctbs * tb_per_ctb + 1) as usize
            },
            min_tb_width: {
                let log2_diff = log2_ctb_size.saturating_sub(sps.min_tb_log2_size_y) as u32;
                let tb_per_ctb = 1u32 << log2_diff;
                let ctb_size = 1u32 << log2_ctb_size;
                let pic_w_in_ctbs = w.div_ceil(ctb_size);
                (pic_w_in_ctbs * tb_per_ctb) as usize
            },
            mc_scratch: match reuse!(mc_scratch) {
                Some(scratch) => scratch,
                None => crate::inter_pred::McScratch::new(),
            },
            row_filter: None,
            decoded_rows: crate::rowfilter::DecodedRows::new(
                (w as usize).div_ceil(1 << log2_ctb_size),
                (h as usize).div_ceil(1 << log2_ctb_size),
            ),
            row_exact: false,
            block_buf: reuse!(block_buf).unwrap_or_default(),
        }
    }

    /// Look up the Z-scan order address for a min-TB at picture position
    /// `(x_tb, y_tb)` in min-TB units. Returns -1 for out-of-bounds (sentinel).
    pub fn zscan_addr(&self, x_tb: i32, y_tb: i32) -> i32 {
        // +1 offset accounts for the sentinel row/column at index 0.
        let col = (x_tb + 1) as usize;
        let row = (y_tb + 1) as usize;
        if col >= self.min_tb_addr_zs_stride
            || row * self.min_tb_addr_zs_stride >= self.min_tb_addr_zs.len()
        {
            return -1;
        }
        self.min_tb_addr_zs[row * self.min_tb_addr_zs_stride + col]
    }

    /// Take ownership of the pixel planes and motion field, wrapping them
    /// as `PixelData` for storage in `DecodedPicture`. Returns
    /// `(y, u, v, tab_mvf, log2_min_pu_size, min_pu_width, log2_ctb_size)`.
    pub fn take_planes_and_mvf(
        &mut self,
    ) -> (
        crate::pixel::PixelData,
        crate::pixel::PixelData,
        crate::pixel::PixelData,
        Vec<MvField>,
        u8,
        usize,
        u8,
    ) {
        let y = P::wrap_vec(std::mem::take(&mut self.y_plane).into_vec());
        let u = P::wrap_vec(std::mem::take(&mut self.u_plane).into_vec());
        let v = P::wrap_vec(std::mem::take(&mut self.v_plane).into_vec());
        let tab_mvf = std::mem::take(&mut self.tab_mvf).into_vec();
        (
            y,
            u,
            v,
            tab_mvf,
            self.log2_min_pu_size,
            self.min_pu_width,
            self.log2_ctb_size,
        )
    }
}

/// Build the min-TB Z-scan order table (HEVC spec 6.5.1, FFmpeg
/// `pps->min_tb_addr_zs`) for an SPS. It depends only on the picture size,
/// CTB size and min TB size, so callers decoding many pictures should build
/// it once and share it.
pub fn build_min_tb_addr_zs(sps: &Sps) -> Vec<i32> {
    let log2_ctb_size = sps.ctb_log2_size_y;
    let w = sps.pic_width_in_luma_samples;
    let h = sps.pic_height_in_luma_samples;
    // Build Z-scan order table (HEVC spec 6.5.1, FFmpeg min_tb_addr_zs).
    // tb_mask = number of min-TBs per CTB side - 1
    let log2_diff = log2_ctb_size.saturating_sub(sps.min_tb_log2_size_y) as u32;
    let tb_per_ctb = 1u32 << log2_diff; // min-TBs per CTB side
    let tb_mask = tb_per_ctb - 1;
    let ctb_size = 1u32 << log2_ctb_size;
    let pic_w_in_ctbs = w.div_ceil(ctb_size);
    let pic_h_in_ctbs = h.div_ceil(ctb_size);
    let pic_w_in_tbs = pic_w_in_ctbs * tb_per_ctb;
    let pic_h_in_tbs = pic_h_in_ctbs * tb_per_ctb;
    // Table stride includes a -1 sentinel column at index -1,
    // represented as column 0 in the raw array. Accessible range:
    // y in -1..pic_h_in_tbs, x in -1..pic_w_in_tbs.
    let stride = (pic_w_in_tbs + 1) as usize;
    let rows = (pic_h_in_tbs + 1) as usize;
    let mut tab = vec![-1i32; stride * rows];
    // Fill the table (skip sentinel row 0 and column 0).
    for y in 0..pic_h_in_tbs {
        for x in 0..pic_w_in_tbs {
            let tb_x = x >> log2_diff; // CTB column
            let tb_y = y >> log2_diff; // CTB row
            let rs = pic_w_in_ctbs * tb_y + tb_x;
            // For single-tile: ts = rs. For tiles: use
            // ctb_addr_rs_to_ts. We'll use rs directly (tiles
            // are handled via tab_tile_id checks elsewhere).
            let mut val = (rs << (log2_diff * 2)) as i32;
            // Z-order interleave of within-CTB coordinates.
            let lx = x & tb_mask;
            let ly = y & tb_mask;
            for i in 0..log2_diff {
                let m = 1u32 << i;
                if lx & m != 0 {
                    val += (m * m) as i32;
                }
                if ly & m != 0 {
                    val += (2 * m * m) as i32;
                }
            }
            // +1 offset for the sentinel row/column.
            tab[(y as usize + 1) * stride + (x as usize + 1)] = val;
        }
    }
    tab
}

/// Recursive coding tree decode (HEVC spec 7.3.8.4).
///
/// `slice_qp_y` is needed to compute the per-CU effective QP for dequant
/// (`qp_y = slice_qp_y + cu_qp_delta`).
///
/// Returns `Ok(true)` if there is more data in the slice (= `end_of_slice_flag`
/// was 0 at the CTB boundary, OR we haven't reached one yet), or `Ok(false)`
/// if we've consumed the slice's terminate bin and the slice is done.
#[allow(clippy::too_many_arguments)]
pub fn decode_coding_quadtree<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    slice_qp_y: i32,
    slice_params: &SliceParams,
    x0: u32,
    y0: u32,
    log2_cb_size: u8,
    cb_depth: u8,
) -> Result<bool, DecodeError> {
    let cb_size = 1u32 << log2_cb_size;

    // Reset cu_qp_delta coding state at the top of each QP group
    // (spec 7.3.8.4, FFmpeg hls_coding_quadtree). A QP group is a
    // block of size CtbLog2SizeY - diff_cu_qp_delta_depth. When we
    // enter a node at or above that size, reset the "already coded"
    // sentinel so the first TU with non-zero CBF in this group will
    // signal a fresh cu_qp_delta.
    if pps.cu_qp_delta_enabled_flag
        && log2_cb_size
            >= sps
                .ctb_log2_size_y
                .saturating_sub(pps.diff_cu_qp_delta_depth as u8)
    {
        state.is_cu_qp_delta_coded = false;
        state.last_cu_qp_delta = 0;
    }
    // Chroma QP offset groups (spec 7.3.8.4): cu_chroma_qp_offset_flag is
    // coded at most once per group of Log2MinCuChromaQpOffsetSize. The
    // offsets themselves persist until coded again (reset per slice).
    if slice_params.cu_chroma_qp_offset_enabled_flag
        && log2_cb_size
            >= sps
                .ctb_log2_size_y
                .saturating_sub(pps.diff_cu_chroma_qp_offset_depth as u8)
    {
        state.is_cu_chroma_qp_offset_coded = false;
    }

    // Implicit-no-split when we're at min CB size or when the CU would
    // overflow the picture (spec 7.3.8.4).
    let split_cu = if x0 + cb_size <= state.width
        && y0 + cb_size <= state.height
        && log2_cb_size > state.log2_min_cb_size
    {
        decode_split_cu_flag(cabac, contexts, state, x0, y0, cb_depth)? != 0
    } else {
        log2_cb_size > state.log2_min_cb_size
    };

    let more_data;
    if split_cu {
        let cb_size_split = cb_size >> 1;
        let x1 = x0 + cb_size_split;
        let y1 = y0 + cb_size_split;
        let mut md = decode_coding_quadtree(
            cabac,
            contexts,
            state,
            sps,
            pps,
            slice_qp_y,
            slice_params,
            x0,
            y0,
            log2_cb_size - 1,
            cb_depth + 1,
        )?;
        if md && x1 < state.width {
            md = decode_coding_quadtree(
                cabac,
                contexts,
                state,
                sps,
                pps,
                slice_qp_y,
                slice_params,
                x1,
                y0,
                log2_cb_size - 1,
                cb_depth + 1,
            )?;
        }
        if md && y1 < state.height {
            md = decode_coding_quadtree(
                cabac,
                contexts,
                state,
                sps,
                pps,
                slice_qp_y,
                slice_params,
                x0,
                y1,
                log2_cb_size - 1,
                cb_depth + 1,
            )?;
        }
        if md && x1 < state.width && y1 < state.height {
            md = decode_coding_quadtree(
                cabac,
                contexts,
                state,
                sps,
                pps,
                slice_qp_y,
                slice_params,
                x1,
                y1,
                log2_cb_size - 1,
                cb_depth + 1,
            )?;
        }
        // FFmpeg hls_coding_quadtree: when the four sub-CUs all returned
        // more_data=true, check whether there are still CUs beyond this
        // split block inside the picture.  If the bottom-right corner of
        // this block is at or past the picture edge in both dimensions,
        // no terminate bin was decoded for the last leaf, so propagate
        // "no more data" upward to the CTB loop.
        if md {
            more_data = (x1 + cb_size_split) < state.width || (y1 + cb_size_split) < state.height;
        } else {
            more_data = false;
        }
        // Spec 8.6.1 / FFmpeg hls_coding_quadtree:2671-2673: at the
        // bottom-right of a QP-group-aligned split node, snapshot
        // `last_qp_y` into `qpy_pred` for the next group.
        maybe_save_qpy_pred(state, sps, pps, x0, y0, log2_cb_size);
    } else {
        decode_coding_unit(
            cabac,
            contexts,
            state,
            sps,
            pps,
            slice_qp_y,
            slice_params,
            x0,
            y0,
            log2_cb_size,
            cb_depth,
        )?;

        // After a leaf CU, decode end_of_slice_flag if we're at a CTB
        // boundary (or picture edge). Spec 7.3.8.5.
        let ctb_size = 1u32 << state.log2_ctb_size;
        let at_ctb_x_edge =
            (x0 + cb_size).is_multiple_of(ctb_size) || (x0 + cb_size >= state.width);
        let at_ctb_y_edge =
            (y0 + cb_size).is_multiple_of(ctb_size) || (y0 + cb_size >= state.height);
        if at_ctb_x_edge && at_ctb_y_edge {
            let end_of_slice = cabac.decode_terminate();
            more_data = end_of_slice == 0;
        } else {
            more_data = true;
        }
        // For leaf (non-split) CUs, record the coding-tree depth in
        // `tab_ct_depth` for split_cu_flag context derivation of neighbors.
        // This must NOT be called for the split (parent) node — only for
        // leaves — otherwise the parent depth overwrites the children's
        // finer depths (matching FFmpeg: set_ct_depth is inside
        // hls_coding_unit, not hls_coding_quadtree).
        set_ct_depth(state, x0, y0, log2_cb_size, cb_depth);
    }

    Ok(more_data)
}

/// `split_cu_flag` neighbor context derivation (HEVC spec 9.3.4.2.2).
///
/// `inc = (depth_left > cb_depth) + (depth_top > cb_depth)`. Neighbor depths
/// come from `tab_ct_depth`. A neighbor is unavailable (depth 0) if it's
/// outside the picture or in a different slice/tile (spec 6.4.1).
fn decode_split_cu_flag<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &PictureState<P>,
    x0: u32,
    y0: u32,
    cb_depth: u8,
) -> Result<u32, DecodeError> {
    let x_cb = (x0 >> state.log2_min_cb_size) as usize;
    let y_cb = (y0 >> state.log2_min_cb_size) as usize;
    let depth_left = if x_cb > 0 && neighbour_available(state, x0, y0, x0 as i64 - 1, y0 as i64) {
        state.tab_ct_depth[y_cb * state.min_cb_width + x_cb - 1]
    } else {
        0
    };
    let depth_top = if y_cb > 0 && neighbour_available(state, x0, y0, x0 as i64, y0 as i64 - 1) {
        state.tab_ct_depth[(y_cb - 1) * state.min_cb_width + x_cb]
    } else {
        0
    };

    let mut inc = 0usize;
    if depth_left > cb_depth {
        inc += 1;
    }
    if depth_top > cb_depth {
        inc += 1;
    }

    Ok(cabac.decode_bin(&mut contexts.state[ctx::SPLIT_CODING_UNIT_FLAG + inc]))
}

fn set_ct_depth<P: Pixel>(
    state: &mut PictureState<P>,
    x0: u32,
    y0: u32,
    log2_cb_size: u8,
    cb_depth: u8,
) {
    let length = ((1u32 << log2_cb_size) >> state.log2_min_cb_size) as usize;
    let x_cb = (x0 >> state.log2_min_cb_size) as usize;
    let y_cb = (y0 >> state.log2_min_cb_size) as usize;
    for j in 0..length {
        let row = (y_cb + j) * state.min_cb_width;
        for i in 0..length {
            state.tab_ct_depth[row + x_cb + i] = cb_depth;
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3d-3: inter syntax element decoders
// ---------------------------------------------------------------------------

/// Whether the neighbouring luma location `(xn, yn)` is available to the
/// block at `(x0, y0)` for CABAC context selection and intra mode
/// prediction (spec 6.4.1, FFmpeg `ctb_left_flag` / `ctb_up_flag`). It must
/// be inside the picture, and either in the current CTB or in a CTB that
/// belongs to the same slice and the same tile.
#[inline]
fn neighbour_available<P: Pixel>(
    state: &PictureState<P>,
    x0: u32,
    y0: u32,
    xn: i64,
    yn: i64,
) -> bool {
    if xn < 0 || yn < 0 || xn >= state.width as i64 || yn >= state.height as i64 {
        return false;
    }
    let log2_ctb = state.log2_ctb_size as u32;
    let (cx, cy) = ((x0 >> log2_ctb) as usize, (y0 >> log2_ctb) as usize);
    let (nx, ny) = (
        (xn as u32 >> log2_ctb) as usize,
        (yn as u32 >> log2_ctb) as usize,
    );
    if (cx, cy) == (nx, ny) {
        return true;
    }
    let ctb_w = state.width_in_ctbs;
    let (cur, nb) = (cy * ctb_w + cx, ny * ctb_w + nx);
    let slice = |rs: usize| state.tab_slice_addr_rs.get(rs).copied().unwrap_or(-1);
    let tile = |rs: usize| state.tab_tile_id.get(rs).copied();
    slice(nb) >= 0 && slice(nb) == slice(cur) && tile(nb) == tile(cur)
}

/// `cu_skip_flag` decode (FFmpeg `ff_hevc_skip_flag_decode`).
/// Context: SKIP_FLAG offset + (skip_left + skip_above).
fn decode_skip_flag<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &PictureState<P>,
    x0: u32,
    y0: u32,
) -> u32 {
    let x_cb = (x0 >> state.log2_min_cb_size) as usize;
    let y_cb = (y0 >> state.log2_min_cb_size) as usize;
    let mut inc = 0usize;
    if x_cb > 0 && neighbour_available(state, x0, y0, x0 as i64 - 1, y0 as i64) {
        inc += (state.tab_skip_flag[y_cb * state.min_cb_width + x_cb - 1] != 0) as usize;
    }
    if y_cb > 0 && neighbour_available(state, x0, y0, x0 as i64, y0 as i64 - 1) {
        inc += (state.tab_skip_flag[(y_cb - 1) * state.min_cb_width + x_cb] != 0) as usize;
    }
    cabac.decode_bin(&mut contexts.state[ctx::SKIP_FLAG + inc])
}

/// `part_mode` decode for inter CUs (FFmpeg `ff_hevc_part_mode_decode`).
/// Extended to cover all 8 partition modes.
fn decode_part_mode(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    sps: &Sps,
    log2_cb_size: u8,
    pred_mode: PredMode,
) -> PartMode {
    if cabac.decode_bin(&mut contexts.state[ctx::PART_MODE]) != 0 {
        return PartMode::Part2Nx2N;
    }
    if log2_cb_size == sps.min_cb_log2_size_y {
        if pred_mode == PredMode::Intra {
            return PartMode::PartNxN;
        }
        if cabac.decode_bin(&mut contexts.state[ctx::PART_MODE + 1]) != 0 {
            return PartMode::Part2NxN;
        }
        if log2_cb_size == 3 {
            return PartMode::PartNx2N;
        }
        if cabac.decode_bin(&mut contexts.state[ctx::PART_MODE + 2]) != 0 {
            return PartMode::PartNx2N;
        }
        return PartMode::PartNxN;
    }

    // log2_cb_size > min_cb_log2_size.
    if !sps.amp_enabled_flag {
        if cabac.decode_bin(&mut contexts.state[ctx::PART_MODE + 1]) != 0 {
            return PartMode::Part2NxN;
        }
        return PartMode::PartNx2N;
    }

    // AMP enabled.
    if cabac.decode_bin(&mut contexts.state[ctx::PART_MODE + 1]) != 0 {
        if cabac.decode_bin(&mut contexts.state[ctx::PART_MODE + 3]) != 0 {
            return PartMode::Part2NxN;
        }
        if cabac.decode_bypass() != 0 {
            return PartMode::Part2NxnD;
        }
        return PartMode::Part2NxnU;
    }

    if cabac.decode_bin(&mut contexts.state[ctx::PART_MODE + 3]) != 0 {
        return PartMode::PartNx2N;
    }
    if cabac.decode_bypass() != 0 {
        return PartMode::PartnRx2N;
    }
    PartMode::PartnLx2N
}

// ---------------------------------------------------------------------------
// Merge candidate list construction (spec 8.5.3.2.2 / 8.5.3.2.3)
// ---------------------------------------------------------------------------

/// Check if two positions belong to the same parallel merge region.
/// When `log2_parallel_merge_level > 2`, small PUs inside the region share
/// a single merge candidate list and must not reference each other (spec
/// 8.5.3.2.2, `is_diff_mer` in FFmpeg).
fn is_diff_mer(log2_parallel_merge_level: u8, x_n: i32, y_n: i32, x_p: i32, y_p: i32) -> bool {
    let pl = log2_parallel_merge_level;
    if pl >= 32 {
        return false; // merge region covers the entire picture
    }
    (x_n >> pl) == (x_p >> pl) && (y_n >> pl) == (y_p >> pl)
}

/// Read the MvField from `tab_mvf` at luma sample position `(x, y)`.
fn tab_mvf_at<P: Pixel>(state: &PictureState<P>, x: i32, y: i32) -> MvField {
    let x_pu = (x as u32 >> state.log2_min_pu_size) as usize;
    let y_pu = (y as u32 >> state.log2_min_pu_size) as usize;
    state.tab_mvf[y_pu * state.min_pu_width + x_pu]
}

/// Whether the A0 / B0 candidate at `(x_n, y_n)` is decoded before the PU
/// at `(x0, y0)` (spec 6.4.1 z-scan order; FFmpeg `PRED_BLOCK_AVAILABLE`).
/// Within a tile, raster CTB order is decode order, so the picture-wide
/// z-scan table orders them (the caller has checked slice and tile).
/// Without it, a below-left or later above-right position is taken as
/// available whenever its motion field holds an inter PU, as it can after a
/// slice overlapping an earlier one in a damaged stream. Only for A0 / B0:
/// A1, B1 and B2 always precede the PU or lie in its own coding block,
/// where spec 6.4.2 makes them available whatever their z-scan order.
fn zscan_available<P: Pixel>(
    state: &PictureState<P>,
    x0: i32,
    y0: i32,
    x_n: i32,
    y_n: i32,
) -> bool {
    let s = state.log2_min_tb_size as i32;
    state.zscan_addr(x_n >> s, y_n >> s) <= state.zscan_addr(x0 >> s, y0 >> s)
}

/// Check whether the neighbor at `(x_n, y_n)` is available as a spatial
/// merge candidate for a PU at `(x0, y0)`.  The position must be inside
/// the picture, belong to an inter-coded PU (`pred_flag != 0`), be in the
/// same slice and same tile, and already decoded (z-scan order).
fn spatial_cand_available<P: Pixel>(
    state: &PictureState<P>,
    x0: i32,
    y0: i32,
    x_n: i32,
    y_n: i32,
) -> bool {
    // Out of picture bounds?
    if x_n < 0 || y_n < 0 || x_n >= state.width as i32 || y_n >= state.height as i32 {
        return false;
    }
    // Same slice?
    let log2_ctb = state.log2_ctb_size;
    let ctb_w = state.width_in_ctbs;
    let curr_ctb_rs = (y0 as usize >> log2_ctb) * ctb_w + (x0 as usize >> log2_ctb);
    let n_ctb_rs = (y_n as usize >> log2_ctb) * ctb_w + (x_n as usize >> log2_ctb);
    if state.tab_slice_addr_rs[n_ctb_rs] != state.tab_slice_addr_rs[curr_ctb_rs] {
        return false;
    }
    // Same tile?
    if state.tab_tile_id[n_ctb_rs] != state.tab_tile_id[curr_ctb_rs] {
        return false;
    }
    // The PU at that position must be inter (pred_flag != 0).
    let mvf = tab_mvf_at(state, x_n, y_n);
    mvf.pred_flag != 0
}

/// Derives the selected merge candidate for a PU (HEVC spec 8.5.3.2.2).
///
/// Only the four candidates before the selected index can be needed for
/// duplicate checks and B-slice combinations.
///
/// `single_mcl_flag` is set when `log2_parallel_merge_level > 2` and
/// `cb_size == 8`; in that case the candidate list is derived once for the
/// whole CU (part_idx forced to 0, PU size = CU size).
#[allow(clippy::too_many_arguments)]
fn select_merge_candidate<P: Pixel>(
    state: &PictureState<P>,
    slice_params: &SliceParams,
    x0: u32,
    y0: u32,
    n_pb_w: u32,
    n_pb_h: u32,
    merge_idx: u32,
    part_mode: PartMode,
    part_idx: u8,
    single_mcl_flag: bool,
) -> MvField {
    let selected = merge_idx.min(slice_params.max_num_merge_cand - 1) as usize;
    let mut list = [MvField::default(); 4];
    let mut count = 0;
    macro_rules! add_candidate {
        ($mvf:expr) => {{
            let candidate = $mvf;
            if count == selected {
                return candidate;
            }
            list[count] = candidate;
            count += 1;
        }};
    }

    let x0i = x0 as i32;
    let y0i = y0 as i32;
    let w = n_pb_w as i32;
    let h = n_pb_h as i32;
    let pl = slice_params.log2_parallel_merge_level;

    // Spatial candidate positions (spec table 8-11).
    let x_a1 = x0i - 1;
    let y_a1 = y0i + h - 1;
    let x_b1 = x0i + w - 1;
    let y_b1 = y0i - 1;
    let x_b0 = x0i + w;
    let y_b0 = y0i - 1;
    let x_a0 = x0i - 1;
    let y_a0 = y0i + h;
    let x_b2 = x0i - 1;
    let y_b2 = y0i - 1;

    // --- A1 (left) ---
    let is_available_a1 = if !single_mcl_flag
        && part_idx == 1
        && matches!(
            part_mode,
            PartMode::PartNx2N | PartMode::PartnLx2N | PartMode::PartnRx2N
        )
        || is_diff_mer(pl, x_a1, y_a1, x0i, y0i)
    {
        false
    } else {
        spatial_cand_available(state, x0i, y0i, x_a1, y_a1)
    };
    if is_available_a1 {
        add_candidate!(tab_mvf_at(state, x_a1, y_a1));
    }

    // --- B1 (above) ---
    // is_available_b1 is the RAW spatial availability (without A1 prune).
    // Used by B0 and B2 for their prune checks. The A1 prune only gates
    // whether B1 is added to the list. Matches FFmpeg where is_available_b1
    // stays true even when B1 is not added due to the A1 prune.
    let is_available_b1 = if !single_mcl_flag
        && part_idx == 1
        && matches!(
            part_mode,
            PartMode::Part2NxN | PartMode::Part2NxnU | PartMode::Part2NxnD
        )
        || is_diff_mer(pl, x_b1, y_b1, x0i, y0i)
    {
        false
    } else {
        spatial_cand_available(state, x0i, y0i, x_b1, y_b1)
    };
    if is_available_b1
        && !(is_available_a1
            && tab_mvf_at(state, x_b1, y_b1).merge_eq(&tab_mvf_at(state, x_a1, y_a1)))
    {
        add_candidate!(tab_mvf_at(state, x_b1, y_b1));
    }

    // --- B0 (above-right) ---
    let is_available_b0 = spatial_cand_available(state, x0i, y0i, x_b0, y_b0)
        && zscan_available(state, x0i, y0i, x_b0, y_b0)
        && x_b0 < state.width as i32
        && !is_diff_mer(pl, x_b0, y_b0, x0i, y0i)
        && !(is_available_b1
            && tab_mvf_at(state, x_b0, y_b0).merge_eq(&tab_mvf_at(state, x_b1, y_b1)));
    if is_available_b0 {
        add_candidate!(tab_mvf_at(state, x_b0, y_b0));
    }

    // --- A0 (below-left) ---
    let is_available_a0 = spatial_cand_available(state, x0i, y0i, x_a0, y_a0)
        && zscan_available(state, x0i, y0i, x_a0, y_a0)
        && y_a0 < state.height as i32
        && !is_diff_mer(pl, x_a0, y_a0, x0i, y0i)
        && !(is_available_a1
            && tab_mvf_at(state, x_a0, y_a0).merge_eq(&tab_mvf_at(state, x_a1, y_a1)));
    if is_available_a0 {
        add_candidate!(tab_mvf_at(state, x_a0, y_a0));
    }

    // --- B2 (above-left) — only if fewer than 4 candidates so far ---
    if count < 4 {
        #[allow(clippy::nonminimal_bool)]
        let is_available_b2 = spatial_cand_available(state, x0i, y0i, x_b2, y_b2)
            && !is_diff_mer(pl, x_b2, y_b2, x0i, y0i)
            && !(is_available_a1
                && tab_mvf_at(state, x_b2, y_b2).merge_eq(&tab_mvf_at(state, x_a1, y_a1)))
            && !(is_available_b1
                && tab_mvf_at(state, x_b2, y_b2).merge_eq(&tab_mvf_at(state, x_b1, y_b1)));
        if is_available_b2 {
            add_candidate!(tab_mvf_at(state, x_b2, y_b2));
        }
    }

    // --- Temporal merge candidate (spec 8.5.3.2.3) ---
    if slice_params.slice_temporal_mvp_enabled_flag {
        let mut mv_l0_col = Mv::default();
        let mut mv_l1_col = Mv::default();
        let available_l0 = temporal_luma_motion_vector(
            state,
            slice_params,
            x0i,
            y0i,
            w,
            h,
            0, // ref_idx = 0 for merge temporal candidate
            0, // L0
        );
        let available_l1 = if slice_params.slice_type == SliceType::B {
            temporal_luma_motion_vector(state, slice_params, x0i, y0i, w, h, 0, 1)
        } else {
            None
        };

        if available_l0.is_some() || available_l1.is_some() {
            if let Some(mv) = available_l0 {
                mv_l0_col = mv;
            }
            if let Some(mv) = available_l1 {
                mv_l1_col = mv;
            }
            let pred_flag = available_l0.is_some() as u8 | ((available_l1.is_some() as u8) << 1);
            add_candidate!(MvField {
                mv: [mv_l0_col, mv_l1_col],
                ref_idx: [0, 0],
                pred_flag,
            });
        }
    }

    // --- Combined bi-predictive candidates (B-slice only, spec 8.5.3.2.4) ---
    let nb_orig_merge_cand = count;
    if slice_params.slice_type == SliceType::B && nb_orig_merge_cand > 1 {
        // Table of (l0_cand_idx, l1_cand_idx) pairs — matches FFmpeg's
        // `l0_l1_cand_idx` table for up to 4 original candidates.
        const L0_L1_CAND_IDX: [[usize; 2]; 12] = [
            [0, 1],
            [1, 0],
            [0, 2],
            [2, 0],
            [1, 2],
            [2, 1],
            [0, 3],
            [3, 0],
            [1, 3],
            [3, 1],
            [2, 3],
            [3, 2],
        ];
        let max_comb = nb_orig_merge_cand * (nb_orig_merge_cand - 1);
        for entry in L0_L1_CAND_IDX.iter().take(max_comb.min(12)) {
            let l0_idx = entry[0];
            let l1_idx = entry[1];
            if l0_idx >= nb_orig_merge_cand || l1_idx >= nb_orig_merge_cand {
                continue;
            }
            let l0_cand = list[l0_idx];
            let l1_cand = list[l1_idx];

            if (l0_cand.pred_flag & 1 != 0)
                && (l1_cand.pred_flag & 2 != 0)
                && (slice_params.ref_pic_list_pocs[0].get(l0_cand.ref_idx[0] as usize)
                    != slice_params.ref_pic_list_pocs[1].get(l1_cand.ref_idx[1] as usize)
                    || l0_cand.mv[0] != l1_cand.mv[1])
            {
                add_candidate!(MvField {
                    mv: [l0_cand.mv[0], l1_cand.mv[1]],
                    ref_idx: [l0_cand.ref_idx[0], l1_cand.ref_idx[1]],
                    pred_flag: 3, // PF_BI
                });
            }
        }
    }

    // Remaining candidates are zero vectors in reference-list order.
    let nb_refs = if slice_params.slice_type == SliceType::P {
        slice_params.num_ref_idx_l0_active
    } else {
        slice_params
            .num_ref_idx_l0_active
            .min(slice_params.num_ref_idx_l1_active)
    };
    let zero_idx = (selected - count) as u32;
    let reference = if zero_idx < nb_refs {
        zero_idx as i8
    } else {
        0
    };
    let pred_flag = if slice_params.slice_type == SliceType::B {
        3
    } else {
        1
    };
    MvField {
        mv: [Mv::default(); 2],
        ref_idx: [reference; 2],
        pred_flag,
    }
}

/// MV scaling per HEVC spec 8.5.3.2.8 / FFmpeg `mv_scale`.
///
/// `td` = `poc_col - poc_col_ref` (temporal distance of the neighbor's MV)
/// `tb` = `poc_curr - poc_ref_curr` (temporal distance of the current ref)
///
/// Returns the scaled MV. The rounding follows the FFmpeg implementation
/// which adds `(scale * mv < 0)` before shifting — equivalent to "round
/// away from zero" for positive scale, matching the spec.
fn mv_scale(mv: Mv, td: i32, tb: i32) -> Mv {
    let td = td.clamp(-128, 127);
    let tb = tb.clamp(-128, 127);
    if td == 0 {
        return mv;
    }
    let tx = (0x4000 + (td.abs() >> 1)) / td;
    let scale_factor = ((tb * tx + 32) >> 6).clamp(-4096, 4095);
    let sx = (scale_factor * mv.x as i32 + 127 + ((scale_factor * mv.x as i32) < 0) as i32) >> 8;
    let sy = (scale_factor * mv.y as i32 + 127 + ((scale_factor * mv.y as i32) < 0) as i32) >> 8;
    Mv {
        x: sx.clamp(-32768, 32767) as i16,
        y: sy.clamp(-32768, 32767) as i16,
    }
}

/// Phase 3e: check_mvset — derive temporal colocated MV from a specific
/// reference list of the collocated PU (spec 8.5.3.2.8).
///
/// `col_mv` = the collocated PU's MV on list `list_col`.
/// `col_ref_idx` = the collocated PU's ref_idx on list `list_col`.
/// `col_pic_poc` = POC of the collocated picture.
/// `col_ref_pocs` = ref_pic_list_pocs of the collocated picture (list `list_col`).
/// `curr_poc` = POC of the current picture.
/// `curr_ref_poc` = POC of the current picture's reference at `ref_idx_lx` on list `x`.
///
/// Returns `Some(scaled_mv)` if valid, `None` if long-term mismatch etc.
#[allow(clippy::too_many_arguments)]
fn check_mvset(
    col_mv: Mv,
    col_ref_idx: i8,
    col_pic_poc: i32,
    col_ref_pocs: &[i32],
    col_ref_lt: &[bool],
    curr_poc: i32,
    curr_ref_poc: i32,
    curr_ref_lt: bool,
) -> Option<Mv> {
    // If the collocated ref_idx is out of range, bail.
    if col_ref_idx < 0 || (col_ref_idx as usize) >= col_ref_pocs.len() {
        return None;
    }
    let col_ref_poc = col_ref_pocs[col_ref_idx as usize];
    // Spec 8.5.3.2.8: no candidate when exactly one of the two references
    // is long-term; long-term MVs are never scaled.
    let col_lt = col_ref_lt
        .get(col_ref_idx as usize)
        .copied()
        .unwrap_or(false);
    if col_lt != curr_ref_lt {
        return None;
    }
    // Saturating: a damaged stream's POCs may be far apart, and mv_scale
    // clamps the differences to -128..=127 anyway.
    let col_poc_diff = col_pic_poc.saturating_sub(col_ref_poc);
    let cur_poc_diff = curr_poc.saturating_sub(curr_ref_poc);

    if curr_ref_lt || col_poc_diff == cur_poc_diff || col_poc_diff == 0 {
        Some(col_mv)
    } else {
        Some(mv_scale(col_mv, col_poc_diff, cur_poc_diff))
    }
}

/// Phase 3e: derive temporal colocated MVs (spec 8.5.3.2.8 / FFmpeg
/// `derive_temporal_colocated_mvs`).
///
/// Selects which list's MV from the collocated PU to use, based on
/// pred_flag and the "DiffPicCount" heuristic from the spec.
///
/// `temp_col` = MvField at the collocated position.
/// `ref_idx_lx` = current picture's ref_idx for list `x`.
/// `x` = current list (0 or 1).
/// `col_pic_poc` = POC of the collocated picture.
/// `col_ref_pocs` = the collocated picture's `ref_pic_list_pocs`.
/// `slice_params` = current slice params (for current picture's ref list POCs).
fn derive_temporal_colocated_mvs(
    temp_col: MvField,
    ref_idx_lx: i8,
    x: usize,
    col_pic_poc: i32,
    col_lists: &crate::dpb::SliceRefLists,
    slice_params: &SliceParams,
) -> Option<Mv> {
    let curr_poc = slice_params.poc;
    // A ref_idx beyond the list can only come from a malformed stream;
    // treat the candidate as unavailable rather than panicking.
    let ri = usize::try_from(ref_idx_lx).ok()?;
    let curr_ref_poc = *slice_params.ref_pic_list_pocs[x].get(ri)?;
    let curr_ref_lt = slice_params.ref_pic_list_is_lt[x]
        .get(ri)
        .copied()
        .unwrap_or(false);

    if temp_col.pred_flag == 0 {
        return None;
    }

    if temp_col.pred_flag & 1 == 0 {
        // No L0, use L1.
        return check_mvset(
            temp_col.mv[1],
            temp_col.ref_idx[1],
            col_pic_poc,
            &col_lists.pocs[1],
            &col_lists.is_long_term[1],
            curr_poc,
            curr_ref_poc,
            curr_ref_lt,
        );
    } else if temp_col.pred_flag == 1 {
        // L0 only.
        return check_mvset(
            temp_col.mv[0],
            temp_col.ref_idx[0],
            col_pic_poc,
            &col_lists.pocs[0],
            &col_lists.is_long_term[0],
            curr_poc,
            curr_ref_poc,
            curr_ref_lt,
        );
    }

    // Bi-prediction: choose based on DiffPicCount heuristic.
    // Check if any reference in any list has POC > current POC.
    let has_future_reference = slice_params
        .ref_pic_list_pocs
        .iter()
        .flatten()
        .any(|&ref_poc| ref_poc > curr_poc);

    if !has_future_reference {
        // All references are before current picture: use same list.
        if x == 0 {
            check_mvset(
                temp_col.mv[0],
                temp_col.ref_idx[0],
                col_pic_poc,
                &col_lists.pocs[0],
                &col_lists.is_long_term[0],
                curr_poc,
                curr_ref_poc,
                curr_ref_lt,
            )
        } else {
            check_mvset(
                temp_col.mv[1],
                temp_col.ref_idx[1],
                col_pic_poc,
                &col_lists.pocs[1],
                &col_lists.is_long_term[1],
                curr_poc,
                curr_ref_poc,
                curr_ref_lt,
            )
        }
    } else {
        // Mixed direction: use the opposite of the collocated list.
        // FFmpeg: collocated_list = collocated_from_l0_flag ? 0 : 1.
        // if collocated_list == L1 => CHECK_MVSET(0) (use L0 MV)
        // if collocated_list == L0 => CHECK_MVSET(1) (use L1 MV)
        let col_list = if slice_params.collocated_from_l0_flag {
            0usize
        } else {
            1usize
        };
        if col_list == 1 {
            // Collocated from L1 → use L0 MV of the collocated PU.
            check_mvset(
                temp_col.mv[0],
                temp_col.ref_idx[0],
                col_pic_poc,
                &col_lists.pocs[0],
                &col_lists.is_long_term[0],
                curr_poc,
                curr_ref_poc,
                curr_ref_lt,
            )
        } else {
            // Collocated from L0 → use L1 MV of the collocated PU.
            check_mvset(
                temp_col.mv[1],
                temp_col.ref_idx[1],
                col_pic_poc,
                &col_lists.pocs[1],
                &col_lists.is_long_term[1],
                curr_poc,
                curr_ref_poc,
                curr_ref_lt,
            )
        }
    }
}

/// Phase 3e: temporal luma motion vector prediction (spec 8.5.3.2.3 /
/// FFmpeg `temporal_luma_motion_vector`).
///
/// Attempts to derive a temporal MV candidate from the collocated picture.
/// Returns `Some(mv)` if a valid temporal candidate was found.
#[allow(clippy::too_many_arguments)]
fn temporal_luma_motion_vector<P: Pixel>(
    state: &PictureState<P>,
    slice_params: &SliceParams,
    x0: i32,
    y0: i32,
    n_pb_w: i32,
    n_pb_h: i32,
    ref_idx_lx: i8,
    x: usize, // 0 for L0, 1 for L1
) -> Option<Mv> {
    let col_ref = slice_params.collocated_ref.as_ref()?;
    if col_ref.tab_mvf.is_empty() {
        return None;
    }
    // Frame threading: the collocated picture may still be decoding. Both
    // candidates lie at or above this block's bottom row (the bottom-right
    // one is used only inside this block's CTB row), so the collocated
    // picture's CTB rows down to there must be decoded, counted in its own
    // CTB size (a broken stream can give it another one).
    let bottom = (y0 + n_pb_h).max(0) as usize;
    col_ref
        .progress
        .wait_decoded_ctb_rows((bottom >> col_ref.log2_ctb_size) + 1);

    let col_pic_poc = col_ref.poc;
    let min_pu_width = col_ref.min_pu_width;
    // A position outside the collocated picture (a damaged stream can give
    // it other dimensions: references are cleared when the size changes,
    // but stay defensive) has no motion; indexing its motion field with
    // the current picture's coordinates would wrap into rows not waited for.
    let inside = |x: usize, y: usize| x < col_ref.width as usize && y < col_ref.height as usize;
    let log2_min_pu = col_ref.log2_min_pu_size;
    let log2_ctb = state.log2_ctb_size;

    // Bottom-right collocated position (spec 8.5.3.2.3).
    let xbr = x0 + n_pb_w;
    let ybr = y0 + n_pb_h;

    let mut result: Option<Mv> = None;

    // Try bottom-right first, then center.
    if (y0 >> log2_ctb) == (ybr >> log2_ctb)
        && ybr < state.height as i32
        && xbr < state.width as i32
    {
        // Align to 16-pixel grid (matches FFmpeg `& ~15`).
        let xc = (xbr & !15) as usize;
        let yc = (ybr & !15) as usize;
        let x_pu = xc >> log2_min_pu;
        let y_pu = yc >> log2_min_pu;
        // A collocated CTB without slice lists (lost or failed in a damaged
        // stream) gives no candidate here; the centre is still tried.
        if inside(xc, yc)
            && y_pu * min_pu_width + x_pu < col_ref.tab_mvf.len()
            && let Some(lists) = col_ref.ref_lists_at(xc as u32, yc as u32)
        {
            let temp_col = col_ref.tab_mvf[y_pu * min_pu_width + x_pu];
            result = derive_temporal_colocated_mvs(
                temp_col,
                ref_idx_lx,
                x,
                col_pic_poc,
                lists,
                slice_params,
            );
        }
    }

    // Fallback: center of the PU.
    if result.is_none() {
        let xc = ((x0 + (n_pb_w >> 1)) & !15) as usize;
        let yc = ((y0 + (n_pb_h >> 1)) & !15) as usize;
        let x_pu = xc >> log2_min_pu;
        let y_pu = yc >> log2_min_pu;
        if inside(xc, yc) && y_pu * min_pu_width + x_pu < col_ref.tab_mvf.len() {
            let temp_col = col_ref.tab_mvf[y_pu * min_pu_width + x_pu];
            result = derive_temporal_colocated_mvs(
                temp_col,
                ref_idx_lx,
                x,
                col_pic_poc,
                col_ref.ref_lists_at(xc as u32, yc as u32)?,
                slice_params,
            );
        }
    }

    result
}

/// Check if a spatial neighbor at `(x_n, y_n)` has a MV on list `pred_flag_idx`
/// that references the same picture as `ref_idx` on list `ref_idx_curr` (i.e.
/// same POC). If so, returns `Some(neighbor_mv)`. This corresponds to FFmpeg's
/// `mv_mp_mode_mx`.
fn amvp_same_ref_mv<P: Pixel>(
    state: &PictureState<P>,
    slice_params: &SliceParams,
    x_n: i32,
    y_n: i32,
    pred_flag_idx: usize,
    ref_idx_curr: usize,
    ref_idx: i8,
) -> Option<Mv> {
    let mvf = tab_mvf_at(state, x_n, y_n);
    if (mvf.pred_flag & (1 << pred_flag_idx)) == 0 {
        return None;
    }
    let neighbor_ref_idx = mvf.ref_idx[pred_flag_idx] as usize;
    let neighbor_poc = slice_params.ref_pic_list_pocs[pred_flag_idx]
        .get(neighbor_ref_idx)
        .copied()?;
    let curr_poc = slice_params.ref_pic_list_pocs[ref_idx_curr]
        .get(ref_idx as usize)
        .copied()?;
    if neighbor_poc == curr_poc {
        Some(mvf.mv[pred_flag_idx])
    } else {
        None
    }
}

/// Check if a spatial neighbor at `(x_n, y_n)` has a MV on list `pred_flag_idx`,
/// and if so return that MV scaled by POC distance. This is the "long-term
/// compatible" fallback (FFmpeg `mv_mp_mode_mx_lt`). For short-term refs only.
fn amvp_scaled_ref_mv<P: Pixel>(
    state: &PictureState<P>,
    slice_params: &SliceParams,
    x_n: i32,
    y_n: i32,
    pred_flag_idx: usize,
    ref_idx_curr: usize,
    ref_idx: i8,
) -> Option<Mv> {
    let mvf = tab_mvf_at(state, x_n, y_n);
    if (mvf.pred_flag & (1 << pred_flag_idx)) == 0 {
        return None;
    }
    let neighbor_ref_idx = mvf.ref_idx[pred_flag_idx] as usize;
    let neighbor_ref_poc = slice_params.ref_pic_list_pocs[pred_flag_idx]
        .get(neighbor_ref_idx)
        .copied()?;
    let curr_ref_poc = slice_params.ref_pic_list_pocs[ref_idx_curr]
        .get(ref_idx as usize)
        .copied()?;
    let neighbor_mv = mvf.mv[pred_flag_idx];
    // Spec 8.5.3.2.7: only a neighbour whose reference has the same
    // long-term status as the target qualifies, and long-term MVs are not
    // scaled.
    let lt = |list: usize, idx: usize| {
        slice_params.ref_pic_list_is_lt[list]
            .get(idx)
            .copied()
            .unwrap_or(false)
    };
    let neighbor_lt = lt(pred_flag_idx, neighbor_ref_idx);
    let curr_lt = lt(ref_idx_curr, ref_idx as usize);
    if neighbor_lt != curr_lt {
        return None;
    }
    if curr_lt || neighbor_ref_poc == curr_ref_poc {
        // Same ref → no scaling needed (but this path is the "lt" fallback,
        // FFmpeg still returns the MV here).
        Some(neighbor_mv)
    } else {
        // A neighbour reference with the current POC (only in a damaged
        // stream) scales with td = 1, as FFmpeg's `dist_scale` does.
        let td = match slice_params.poc.saturating_sub(neighbor_ref_poc) {
            0 => 1,
            td => td,
        };
        let tb = slice_params.poc.saturating_sub(curr_ref_poc);
        Some(mv_scale(neighbor_mv, td, tb))
    }
}

/// Try to find an AMVP spatial candidate from a set of positions, first
/// checking for same-ref MVs, then falling back to scaled MVs. Returns
/// `Some(mv)` if a candidate was found.
///
/// `positions` is the list of (x, y) neighbor positions to check (e.g.
/// [A0, A1] for the left group, [B0, B1, B2] for the above group).
#[allow(clippy::too_many_arguments)]
fn amvp_spatial_candidate<P: Pixel>(
    state: &PictureState<P>,
    slice_params: &SliceParams,
    x0: i32,
    y0: i32,
    positions: &[(i32, i32)],
    ref_idx_curr: usize,
    ref_idx: i8,
    same_ref_only: bool,
) -> (Option<Mv>, bool) {
    let pred_flag_l0 = ref_idx_curr;
    let pred_flag_l1 = 1 - ref_idx_curr;

    // Pass 1: same-ref check (exact POC match, no scaling).
    for (i, &(x_n, y_n)) in positions.iter().enumerate() {
        if !spatial_cand_available(state, x0, y0, x_n, y_n)
            || (i == 0 && !zscan_available(state, x0, y0, x_n, y_n))
        {
            continue;
        }
        if let Some(mv) = amvp_same_ref_mv(
            state,
            slice_params,
            x_n,
            y_n,
            pred_flag_l0,
            ref_idx_curr,
            ref_idx,
        ) {
            return (Some(mv), true);
        }
        if let Some(mv) = amvp_same_ref_mv(
            state,
            slice_params,
            x_n,
            y_n,
            pred_flag_l1,
            ref_idx_curr,
            ref_idx,
        ) {
            return (Some(mv), true);
        }
    }

    // Pass 2: scaled-ref fallback. Skipped for the above (B) group when
    // isScaledFlag_L0 is true, per HEVC spec 8.5.3.2.6 step 3: the B
    // candidates are only checked with "LtRefPicList" scaling (steps 3.a–c)
    // when isScaledFlagLX is 0. When isScaledFlagLX is 1, steps 3.a–c
    // are skipped and the B candidates use only the same-ref check from
    // step 2.
    if same_ref_only {
        return (None, false);
    }
    for (i, &(x_n, y_n)) in positions.iter().enumerate() {
        if !spatial_cand_available(state, x0, y0, x_n, y_n)
            || (i == 0 && !zscan_available(state, x0, y0, x_n, y_n))
        {
            continue;
        }
        if let Some(mv) = amvp_scaled_ref_mv(
            state,
            slice_params,
            x_n,
            y_n,
            pred_flag_l0,
            ref_idx_curr,
            ref_idx,
        ) {
            return (Some(mv), true);
        }
        if let Some(mv) = amvp_scaled_ref_mv(
            state,
            slice_params,
            x_n,
            y_n,
            pred_flag_l1,
            ref_idx_curr,
            ref_idx,
        ) {
            return (Some(mv), true);
        }
    }

    (None, false)
}

/// Build the 2-entry AMVP candidate list for a non-merge inter PU.
///
/// Implements HEVC spec 8.5.3.2.6 / FFmpeg `ff_hevc_luma_mv_mvp_mode`.
///
/// `ref_idx` is the decoded `ref_idx_lX` for the current list.
/// `list_idx` is 0 for L0, 1 for L1.
#[allow(clippy::too_many_arguments)]
fn build_amvp_candidates<P: Pixel>(
    state: &PictureState<P>,
    slice_params: &SliceParams,
    x0: u32,
    y0: u32,
    n_pb_w: u32,
    n_pb_h: u32,
    ref_idx: i8,
    list_idx: usize,
) -> [Mv; 2] {
    let mut mvp_list = [Mv::default(); 2];
    let mut num_cand = 0usize;

    let x0i = x0 as i32;
    let y0i = y0 as i32;
    let w = n_pb_w as i32;
    let h = n_pb_h as i32;

    // Left candidates: A0 (below-left), A1 (left).
    let left_positions = [
        (x0i - 1, y0i + h),     // A0
        (x0i - 1, y0i + h - 1), // A1
    ];

    // Check if any left candidate is available (for isScaledFlag_L0).
    let is_scaled_flag_l0 = left_positions.iter().enumerate().any(|(i, &(x_n, y_n))| {
        // Bounds check for A0 (y0 + nPbH may exceed picture height).
        if y_n >= state.height as i32 {
            return false;
        }
        spatial_cand_available(state, x0i, y0i, x_n, y_n)
            && (i != 0 || zscan_available(state, x0i, y0i, x_n, y_n))
    });

    let (left_mv, left_available) = amvp_spatial_candidate(
        state,
        slice_params,
        x0i,
        y0i,
        &left_positions,
        list_idx,
        ref_idx,
        false, // left group always tries both passes
    );

    // Above candidates: B0 (above-right), B1 (above), B2 (above-left).
    let above_positions = [
        (x0i + w, y0i - 1),     // B0
        (x0i + w - 1, y0i - 1), // B1
        (x0i - 1, y0i - 1),     // B2
    ];

    let (above_mv, above_available) = amvp_spatial_candidate(
        state,
        slice_params,
        x0i,
        y0i,
        &above_positions,
        list_idx,
        ref_idx,
        is_scaled_flag_l0, // above group: same-ref only when isScaledFlag_L0
    );

    // FFmpeg `scalef` block: when !isScaledFlag_L0, the above candidate
    // is promoted to the left slot and then B candidates are re-tried with
    // scaling only.
    let (left_mv, left_available, above_mv, above_available) = if !is_scaled_flag_l0 {
        if above_available {
            // Promote above to left.
            let new_left_mv = above_mv;
            let new_left_available = true;
            // Re-derive above with scaled-only pass from B0/B1/B2.
            let pred_flag_l0 = list_idx;
            let pred_flag_l1 = 1 - list_idx;
            let mut new_above_mv = None;
            for (i, &(x_n, y_n)) in above_positions.iter().enumerate() {
                if !spatial_cand_available(state, x0i, y0i, x_n, y_n)
                    || (i == 0 && !zscan_available(state, x0i, y0i, x_n, y_n))
                {
                    continue;
                }
                if let Some(mv) = amvp_scaled_ref_mv(
                    state,
                    slice_params,
                    x_n,
                    y_n,
                    pred_flag_l0,
                    list_idx,
                    ref_idx,
                ) {
                    new_above_mv = Some(mv);
                    break;
                }
                if let Some(mv) = amvp_scaled_ref_mv(
                    state,
                    slice_params,
                    x_n,
                    y_n,
                    pred_flag_l1,
                    list_idx,
                    ref_idx,
                ) {
                    new_above_mv = Some(mv);
                    break;
                }
            }
            (
                new_left_mv,
                new_left_available,
                new_above_mv,
                new_above_mv.is_some(),
            )
        } else {
            (left_mv, left_available, above_mv, above_available)
        }
    } else {
        (left_mv, left_available, above_mv, above_available)
    };

    if left_available {
        mvp_list[num_cand] = left_mv.unwrap_or_default();
        num_cand += 1;
    }

    // Prune: only add above if different from left (or left wasn't available).
    if above_available {
        let above = above_mv.unwrap_or_default();
        if !left_available || above != left_mv.unwrap_or_default() {
            mvp_list[num_cand] = above;
            num_cand += 1;
        }
    }

    // Temporal candidate (spec 8.5.3.2.7).
    if num_cand < 2
        && slice_params.slice_temporal_mvp_enabled_flag
        && let Some(mv_col) = temporal_luma_motion_vector(
            state,
            slice_params,
            x0 as i32,
            y0 as i32,
            n_pb_w as i32,
            n_pb_h as i32,
            ref_idx,
            list_idx,
        )
    {
        mvp_list[num_cand] = mv_col;
        num_cand += 1;
    }

    // Zero-MV fill: pad to exactly 2 candidates.
    // (mvp_list was initialized to Mv::default() = zero, so we just
    // need num_cand == 2 conceptually; the array is already zero-filled.)
    let _ = num_cand;

    mvp_list
}

/// Decode a single prediction unit's merge/AMVP syntax (spec 7.3.8.6 /
/// FFmpeg `hls_prediction_unit`). Returns the `merge_flag` value (needed
/// for `rqt_root_cbf` gating).
#[allow(clippy::too_many_arguments)]
fn decode_prediction_unit<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    slice_params: &SliceParams,
    x0: u32,
    y0: u32,
    n_pb_w: u32,
    n_pb_h: u32,
    log2_cb_size: u8,
    is_skip: bool,
    cb_depth: u8,
    part_mode: PartMode,
    part_idx: u8,
) -> Result<bool, DecodeError> {
    let mut current_mv = MvField::default();
    let merge_flag = if is_skip {
        true
    } else {
        cabac.decode_bin(&mut contexts.state[ctx::MERGE_FLAG]) != 0
    };

    if merge_flag {
        // Merge mode: decode merge_idx.
        let merge_idx = if slice_params.max_num_merge_cand > 1 {
            decode_merge_idx(cabac, contexts, slice_params.max_num_merge_cand)
        } else {
            0
        };

        // Phase 3d-4: build the merge candidate list and select the
        // candidate indicated by merge_idx.
        let n_cs = 1u32 << log2_cb_size;
        let single_mcl_flag = slice_params.log2_parallel_merge_level > 2 && n_cs == 8;
        let (mx0, my0, mw, mh, m_part_idx) = if single_mcl_flag {
            // When singleMCLFlag, derive candidates from the CU origin with
            // PU size = CU size and part_idx = 0 (spec 8.5.3.2.2). (x0, y0)
            // is this PU's origin, which for the second PU of an 8x4 / 4x8
            // split is not the CU origin; CUs are aligned to their size, so
            // round down to the 8x8 grid.
            (x0 & !(n_cs - 1), y0 & !(n_cs - 1), n_cs, n_cs, 0u8)
        } else {
            (x0, y0, n_pb_w, n_pb_h, part_idx)
        };
        current_mv = select_merge_candidate(
            state,
            slice_params,
            mx0,
            my0,
            mw,
            mh,
            merge_idx,
            part_mode,
            m_part_idx,
            single_mcl_flag,
        );

        // Spec: when bi-prediction and the PU is tiny (w+h == 12), demote
        // to L0-only (FFmpeg ff_hevc_luma_mv_merge_mode).
        if current_mv.pred_flag == 3 && (n_pb_w + n_pb_h) == 12 {
            current_mv.pred_flag = 1; // PF_L0
        }
    } else {
        // AMVP mode.
        let inter_pred_idc = if slice_params.slice_type == SliceType::B {
            decode_inter_pred_idc(cabac, contexts, n_pb_w, n_pb_h, cb_depth)
        } else {
            InterPredIdc::PredL0
        };

        // L0.
        if inter_pred_idc != InterPredIdc::PredL1 {
            if slice_params.num_ref_idx_l0_active > 0 {
                current_mv.ref_idx[0] =
                    decode_ref_idx(cabac, contexts, slice_params.num_ref_idx_l0_active, false)
                        as i8;
            }
            current_mv.pred_flag |= 1; // PF_L0
            let mvd = decode_mvd_coding(cabac, contexts);
            let mvp_l0_flag = cabac.decode_bin(&mut contexts.state[ctx::MVP_LX_FLAG]);
            let mvp_list = build_amvp_candidates(
                state,
                slice_params,
                x0,
                y0,
                n_pb_w,
                n_pb_h,
                current_mv.ref_idx[0],
                0,
            );
            let mvp = mvp_list[mvp_l0_flag as usize];
            current_mv.mv[0] = Mv {
                x: mvp.x.wrapping_add(mvd.x),
                y: mvp.y.wrapping_add(mvd.y),
            };
        }

        // L1.
        if inter_pred_idc != InterPredIdc::PredL0 {
            if slice_params.num_ref_idx_l1_active > 0 {
                current_mv.ref_idx[1] =
                    decode_ref_idx(cabac, contexts, slice_params.num_ref_idx_l1_active, true) as i8;
            }
            let mvd = if slice_params.mvd_l1_zero_flag && inter_pred_idc == InterPredIdc::PredBi {
                // mvd_l1_zero_flag: skip MVD coding for L1.
                Mv::default()
            } else {
                decode_mvd_coding(cabac, contexts)
            };
            current_mv.pred_flag |= 2; // PF_L1
            let mvp_l1_flag = cabac.decode_bin(&mut contexts.state[ctx::MVP_LX_FLAG]);
            let mvp_list = build_amvp_candidates(
                state,
                slice_params,
                x0,
                y0,
                n_pb_w,
                n_pb_h,
                current_mv.ref_idx[1],
                1,
            );
            let mvp = mvp_list[mvp_l1_flag as usize];
            current_mv.mv[1] = Mv {
                x: mvp.x.wrapping_add(mvd.x),
                y: mvp.y.wrapping_add(mvd.y),
            };
        }
    }

    // Write the decoded MV field into tab_mvf for all min-PUs in this PU.
    let x_pu = (x0 >> state.log2_min_pu_size) as usize;
    let y_pu = (y0 >> state.log2_min_pu_size) as usize;
    let pu_w = (n_pb_w >> state.log2_min_pu_size).max(1) as usize;
    let pu_h = (n_pb_h >> state.log2_min_pu_size).max(1) as usize;
    for j in 0..pu_h {
        for i in 0..pu_w {
            state.tab_mvf[(y_pu + j) * state.min_pu_width + x_pu + i] = current_mv;
        }
    }

    Ok(merge_flag)
}

/// Decode `merge_idx` (FFmpeg `ff_hevc_merge_idx_decode`).
/// Truncated unary, first bin context-coded, rest bypass.
fn decode_merge_idx(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    max_num_merge_cand: u32,
) -> u32 {
    let mut i = cabac.decode_bin(&mut contexts.state[ctx::MERGE_IDX]);
    if i != 0 {
        while i < max_num_merge_cand - 1 && cabac.decode_bypass() != 0 {
            i += 1;
        }
    }
    i
}

/// Decode `inter_pred_idc` (FFmpeg `ff_hevc_inter_pred_idc_decode`).
///
/// For small blocks (`nPbW + nPbH == 12`), only one context-coded bin
/// (at INTER_PRED_IDC + 4) decides L0 (0) vs L1 (1); BI is not available.
/// For larger blocks, first a bin at INTER_PRED_IDC + ct_depth is read:
/// 1 = BI, 0 = another bin at INTER_PRED_IDC + 4 for L0 vs L1.
fn decode_inter_pred_idc(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    n_pb_w: u32,
    n_pb_h: u32,
    cb_depth: u8,
) -> InterPredIdc {
    if n_pb_w + n_pb_h == 12 {
        let v = cabac.decode_bin(&mut contexts.state[ctx::INTER_PRED_IDC + 4]);
        return if v != 0 {
            InterPredIdc::PredL1
        } else {
            InterPredIdc::PredL0
        };
    }
    if cabac.decode_bin(&mut contexts.state[ctx::INTER_PRED_IDC + cb_depth as usize]) != 0 {
        return InterPredIdc::PredBi;
    }
    let v = cabac.decode_bin(&mut contexts.state[ctx::INTER_PRED_IDC + 4]);
    if v != 0 {
        InterPredIdc::PredL1
    } else {
        InterPredIdc::PredL0
    }
}

/// Decode `ref_idx_lX` (FFmpeg `ff_hevc_ref_idx_lx_decode`).
/// Truncated unary: first 2 bins context-coded, rest bypass.
fn decode_ref_idx(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    num_ref_idx_active: u32,
    is_l1: bool,
) -> u32 {
    let max = num_ref_idx_active - 1;
    if max == 0 {
        return 0;
    }
    // HEVC spec: ref_idx_lX uses the same context model for both L0 and L1.
    // FFmpeg always uses REF_IDX_L0_FLAG contexts regardless of the list.
    let _ = is_l1;
    let ctx_base = ctx::REF_IDX_L0;
    let max_ctx = max.min(2) as usize;
    let mut i = 0u32;
    while (i as usize) < max_ctx
        && cabac.decode_bin(&mut contexts.state[ctx_base + i as usize]) != 0
    {
        i += 1;
    }
    if i == 2 {
        while i < max && cabac.decode_bypass() != 0 {
            i += 1;
        }
    }
    i
}

/// Decode `mvd_coding` (HEVC spec 7.3.8.8 / FFmpeg `ff_hevc_hls_mvd_coding`).
fn decode_mvd_coding(cabac: &mut CabacReader, contexts: &mut CabacContexts) -> Mv {
    let mut x_val = cabac.decode_bin(&mut contexts.state[ctx::ABS_MVD_GREATER0_FLAG]) as i32;
    let mut y_val = cabac.decode_bin(&mut contexts.state[ctx::ABS_MVD_GREATER0_FLAG]) as i32;

    if x_val != 0 {
        x_val += cabac.decode_bin(&mut contexts.state[ctx::ABS_MVD_GREATER1_FLAG + 1]) as i32;
    }
    if y_val != 0 {
        y_val += cabac.decode_bin(&mut contexts.state[ctx::ABS_MVD_GREATER1_FLAG + 1]) as i32;
    }

    let mvd_x = match x_val {
        2 => decode_mvd_abs(cabac),
        1 => decode_mvd_sign(cabac),
        _ => 0,
    };
    let mvd_y = match y_val {
        2 => decode_mvd_abs(cabac),
        1 => decode_mvd_sign(cabac),
        _ => 0,
    };

    Mv {
        x: mvd_x as i16,
        y: mvd_y as i16,
    }
}

/// Decode abs_mvd_minus2 (EG-0 + sign) — FFmpeg `mvd_decode`.
/// Returns the signed MV delta value.
fn decode_mvd_abs(cabac: &mut CabacReader) -> i32 {
    let mut ret: i32 = 2;
    let mut k: u32 = 1;
    const CABAC_MAX_BIN: u32 = 31;
    while k < CABAC_MAX_BIN && cabac.decode_bypass() != 0 {
        ret = ret.saturating_add(1 << k);
        k += 1;
    }
    let mut kk = k;
    while kk > 0 {
        kk -= 1;
        ret = ret.saturating_add((cabac.decode_bypass() as i32) << kk);
    }
    // Sign bit.
    if cabac.decode_bypass() != 0 {
        -ret
    } else {
        ret
    }
}

/// Decode mvd with magnitude 1 (just a sign bit) — FFmpeg `mvd_sign_flag_decode`.
fn decode_mvd_sign(cabac: &mut CabacReader) -> i32 {
    if cabac.decode_bypass() != 0 { -1 } else { 1 }
}

/// Compute inter boundary strength by comparing MVs and reference pictures,
/// per HEVC spec 8.7.2.5 / FFmpeg `boundary_strength`.
///
/// Returns 0 if MVs are close enough (diff < 4 in quarter-pel units), 1 otherwise.
/// Boundary strength between two inter blocks (spec 8.7.2.4): 1 if they
/// use different reference pictures or any MV component differs by 4 or
/// more quarter samples, else 0.
///
/// The common case, identical motion on both sides (every 8×8 grid line
/// inside one prediction unit), is checked inline: with the same reference
/// lists both sides resolve to the same pictures and every |ΔMV| is 0, so
/// the strength is 0. Everything else goes to the out-of-line full check.
#[inline(always)]
fn inter_boundary_strength(
    curr: &MvField,
    neigh: &MvField,
    curr_ref_pocs: &[Vec<i32>; 2],
    neigh_ref_pocs: &[Vec<i32>; 2],
) -> u8 {
    if curr == neigh && std::ptr::eq(curr_ref_pocs, neigh_ref_pocs) {
        0
    } else {
        inter_boundary_strength_full(curr, neigh, curr_ref_pocs, neigh_ref_pocs)
    }
}

#[inline(never)]
fn inter_boundary_strength_full(
    curr: &MvField,
    neigh: &MvField,
    curr_ref_pocs: &[Vec<i32>; 2],
    neigh_ref_pocs: &[Vec<i32>; 2],
) -> u8 {
    // Both bi-pred
    if curr.pred_flag == 3 && neigh.pred_flag == 3 {
        let curr_l0_poc = curr_ref_pocs[0]
            .get(curr.ref_idx[0] as usize)
            .copied()
            .unwrap_or(-1);
        let curr_l1_poc = curr_ref_pocs[1]
            .get(curr.ref_idx[1] as usize)
            .copied()
            .unwrap_or(-1);
        let neigh_l0_poc = neigh_ref_pocs[0]
            .get(neigh.ref_idx[0] as usize)
            .copied()
            .unwrap_or(-1);
        let neigh_l1_poc = neigh_ref_pocs[1]
            .get(neigh.ref_idx[1] as usize)
            .copied()
            .unwrap_or(-1);

        // Same L0 and L1 references for both (all four point to same ref)
        if curr_l0_poc == neigh_l0_poc && curr_l0_poc == curr_l1_poc && neigh_l0_poc == neigh_l1_poc
        {
            // Either ordering must satisfy the threshold
            let order_a = (i32::from(neigh.mv[0].x) - i32::from(curr.mv[0].x)).abs() >= 4
                || (i32::from(neigh.mv[0].y) - i32::from(curr.mv[0].y)).abs() >= 4
                || (i32::from(neigh.mv[1].x) - i32::from(curr.mv[1].x)).abs() >= 4
                || (i32::from(neigh.mv[1].y) - i32::from(curr.mv[1].y)).abs() >= 4;
            let order_b = (i32::from(neigh.mv[1].x) - i32::from(curr.mv[0].x)).abs() >= 4
                || (i32::from(neigh.mv[1].y) - i32::from(curr.mv[0].y)).abs() >= 4
                || (i32::from(neigh.mv[0].x) - i32::from(curr.mv[1].x)).abs() >= 4
                || (i32::from(neigh.mv[0].y) - i32::from(curr.mv[1].y)).abs() >= 4;
            if order_a && order_b { 1 } else { 0 }
        } else if curr_l0_poc == neigh_l0_poc && curr_l1_poc == neigh_l1_poc {
            if (i32::from(neigh.mv[0].x) - i32::from(curr.mv[0].x)).abs() >= 4
                || (i32::from(neigh.mv[0].y) - i32::from(curr.mv[0].y)).abs() >= 4
                || (i32::from(neigh.mv[1].x) - i32::from(curr.mv[1].x)).abs() >= 4
                || (i32::from(neigh.mv[1].y) - i32::from(curr.mv[1].y)).abs() >= 4
            {
                1
            } else {
                0
            }
        } else if curr_l1_poc == neigh_l0_poc && curr_l0_poc == neigh_l1_poc {
            if (i32::from(neigh.mv[1].x) - i32::from(curr.mv[0].x)).abs() >= 4
                || (i32::from(neigh.mv[1].y) - i32::from(curr.mv[0].y)).abs() >= 4
                || (i32::from(neigh.mv[0].x) - i32::from(curr.mv[1].x)).abs() >= 4
                || (i32::from(neigh.mv[0].y) - i32::from(curr.mv[1].y)).abs() >= 4
            {
                1
            } else {
                0
            }
        } else {
            1
        }
    } else if curr.pred_flag != 3 && neigh.pred_flag != 3 {
        // Both uni-pred (one MV each)
        let (a, ref_a_poc) = if curr.pred_flag & 1 != 0 {
            (
                curr.mv[0],
                curr_ref_pocs[0]
                    .get(curr.ref_idx[0] as usize)
                    .copied()
                    .unwrap_or(-1),
            )
        } else {
            (
                curr.mv[1],
                curr_ref_pocs[1]
                    .get(curr.ref_idx[1] as usize)
                    .copied()
                    .unwrap_or(-1),
            )
        };
        let (b, ref_b_poc) = if neigh.pred_flag & 1 != 0 {
            (
                neigh.mv[0],
                neigh_ref_pocs[0]
                    .get(neigh.ref_idx[0] as usize)
                    .copied()
                    .unwrap_or(-1),
            )
        } else {
            (
                neigh.mv[1],
                neigh_ref_pocs[1]
                    .get(neigh.ref_idx[1] as usize)
                    .copied()
                    .unwrap_or(-1),
            )
        };
        if ref_a_poc == ref_b_poc {
            if (i32::from(a.x) - i32::from(b.x)).abs() >= 4
                || (i32::from(a.y) - i32::from(b.y)).abs() >= 4
            {
                1
            } else {
                0
            }
        } else {
            1
        }
    } else {
        // One bi-pred, one uni-pred
        1
    }
}

/// Compute deblocking boundary strengths for a TU or CU, mirroring
/// FFmpeg `ff_hevc_deblocking_boundary_strengths`.
///
/// Called at TU leaf level (for CUs with residual) or at CU level (for skip/no-residual).
///
/// `single_pu`: the block lies in a coding unit with one prediction unit,
/// so the motion inside it is uniform and every internal edge has bS 0.
/// The tables start each picture at 0 and no other block writes these
/// entries, so the internal-edge loops are skipped.
fn compute_deblocking_boundary_strengths<P: Pixel>(
    state: &mut PictureState<P>,
    slice_params: &SliceParams,
    x0: u32,
    y0: u32,
    log2_size: u8,
    single_pu: bool,
) {
    // Edges of CUs in a slice with deblocking disabled are not filtered
    // (spec 8.7.2): leave their boundary strengths at 0.
    if slice_params.deblocking_disabled {
        return;
    }
    let size = 1u32 << log2_size;
    let pic_w = state.width as usize;
    let bs_w = pic_w >> 2;
    let log2_min_pu = state.log2_min_pu_size as u32;
    let min_pu_w = state.min_pu_width;
    let log2_min_tb = state.log2_min_tb_size;
    let min_tb_w = state.min_tb_width;

    // Check if current block is intra
    let x_pu = (x0 >> log2_min_pu) as usize;
    let y_pu = (y0 >> log2_min_pu) as usize;
    let is_intra = state.tab_mvf[y_pu * min_pu_w + x_pu].pred_flag == 0;

    // The current block's reference lists are the current slice's. A
    // neighbour across a slice boundary uses its own slice's lists (spec
    // 8.7.2.4 compares reference pictures, not indices).
    let curr_ref_pocs = &slice_params.ref_pic_list_pocs;
    let log2_ctb = state.log2_ctb_size;
    let ctb_w = state.width_in_ctbs;
    let slice_at = |x: u32, y: u32| -> i32 {
        state.tab_slice_addr_rs[(y as usize >> log2_ctb) * ctb_w + (x as usize >> log2_ctb)]
    };
    let curr_slice = slice_at(x0, y0);
    let ref_pocs_at = |x: u32, y: u32| -> &[Vec<i32>; 2] {
        let addr = slice_at(x, y);
        if addr == curr_slice {
            return curr_ref_pocs;
        }
        slice_params
            .picture_slice_ref_lists
            .iter()
            .rev()
            .find(|(a, _)| *a == addr)
            .map_or(curr_ref_pocs, |(_, l)| &l.pocs)
    };
    let top_ref_pocs = if y0 > 0 {
        ref_pocs_at(x0, y0 - 1)
    } else {
        curr_ref_pocs
    };
    let left_ref_pocs = if x0 > 0 {
        ref_pocs_at(x0 - 1, y0)
    } else {
        curr_ref_pocs
    };

    // Top boundary: y0 > 0 && 8-aligned
    if y0 > 0 && (y0 & 7) == 0 {
        let yp_pu = ((y0 - 1) >> log2_min_pu) as usize;
        let yq_pu = (y0 >> log2_min_pu) as usize;
        let yp_tu = ((y0 - 1) >> log2_min_tb) as usize;
        let yq_tu = (y0 >> log2_min_tb) as usize;

        let yy = (y0 >> 2) as usize;
        for i in (0..size).step_by(4) {
            let x_cur = x0 + i;
            let xx = (x_cur >> 2) as usize;
            let xpu = (x_cur >> log2_min_pu) as usize;
            let xtu = (x_cur >> log2_min_tb) as usize;

            let top_mvf = &state.tab_mvf[yp_pu * min_pu_w + xpu];
            let curr_mvf = &state.tab_mvf[yq_pu * min_pu_w + xpu];

            let bs = if curr_mvf.pred_flag == 0 || top_mvf.pred_flag == 0 {
                // At least one side is intra (pred_flag == 0 means intra in our encoding)
                2
            } else {
                let top_cbf = state.tab_cbf_luma[yp_tu * min_tb_w + xtu];
                let curr_cbf = state.tab_cbf_luma[yq_tu * min_tb_w + xtu];
                if top_cbf != 0 || curr_cbf != 0 {
                    1
                } else {
                    inter_boundary_strength(curr_mvf, top_mvf, curr_ref_pocs, top_ref_pocs)
                }
            };
            state.bs_horizontal[yy * bs_w + xx] = bs;
        }
    }

    // Left boundary: x0 > 0 && 8-aligned
    if x0 > 0 && (x0 & 7) == 0 {
        let xp_pu = ((x0 - 1) >> log2_min_pu) as usize;
        let xq_pu = (x0 >> log2_min_pu) as usize;
        let xp_tu = ((x0 - 1) >> log2_min_tb) as usize;
        let xq_tu = (x0 >> log2_min_tb) as usize;

        let xx = (x0 >> 2) as usize;
        for i in (0..size).step_by(4) {
            let y_cur = y0 + i;
            let yy = (y_cur >> 2) as usize;
            let ypu = (y_cur >> log2_min_pu) as usize;
            let ytu = (y_cur >> log2_min_tb) as usize;

            let left_mvf = &state.tab_mvf[ypu * min_pu_w + xp_pu];
            let curr_mvf = &state.tab_mvf[ypu * min_pu_w + xq_pu];

            let bs = if curr_mvf.pred_flag == 0 || left_mvf.pred_flag == 0 {
                2
            } else {
                let left_cbf = state.tab_cbf_luma[ytu * min_tb_w + xp_tu];
                let curr_cbf = state.tab_cbf_luma[ytu * min_tb_w + xq_tu];
                if left_cbf != 0 || curr_cbf != 0 {
                    1
                } else {
                    inter_boundary_strength(curr_mvf, left_mvf, curr_ref_pocs, left_ref_pocs)
                }
            };
            state.bs_vertical[yy * bs_w + xx] = bs;
        }
    }

    // Internal PU boundaries within a large TU (for inter blocks only)
    if (log2_size as u32) > log2_min_pu && !is_intra && !single_pu {
        // Internal horizontal PU boundaries (every 8 pixels)
        for j in (8..size).step_by(8) {
            let yp_pu = ((y0 + j - 1) >> log2_min_pu) as usize;
            let yq_pu = ((y0 + j) >> log2_min_pu) as usize;
            let yy = ((y0 + j) >> 2) as usize;

            for i in (0..size).step_by(4) {
                let x_cur = x0 + i;
                let xx = (x_cur >> 2) as usize;
                let xpu = (x_cur >> log2_min_pu) as usize;

                let top_mvf = &state.tab_mvf[yp_pu * min_pu_w + xpu];
                let curr_mvf = &state.tab_mvf[yq_pu * min_pu_w + xpu];

                let bs = inter_boundary_strength(curr_mvf, top_mvf, curr_ref_pocs, curr_ref_pocs);
                state.bs_horizontal[yy * bs_w + xx] = bs;
            }
        }

        // Internal vertical PU boundaries (every 8 pixels)
        for j in (0..size).step_by(4) {
            let ypu = ((y0 + j) >> log2_min_pu) as usize;
            let yy = ((y0 + j) >> 2) as usize;

            for i in (8..size).step_by(8) {
                let xp_pu = ((x0 + i - 1) >> log2_min_pu) as usize;
                let xq_pu = ((x0 + i) >> log2_min_pu) as usize;
                let xx = ((x0 + i) >> 2) as usize;

                let left_mvf = &state.tab_mvf[ypu * min_pu_w + xp_pu];
                let curr_mvf = &state.tab_mvf[ypu * min_pu_w + xq_pu];

                let bs = inter_boundary_strength(curr_mvf, left_mvf, curr_ref_pocs, curr_ref_pocs);
                state.bs_vertical[yy * bs_w + xx] = bs;
            }
        }
    }
}

/// `coding_unit` decode (HEVC spec 7.3.8.5 / FFmpeg `hls_coding_unit`).
///
/// Handles both intra (I/P/B slices) and inter (P/B slices) CUs.
#[allow(clippy::too_many_arguments)]
fn decode_coding_unit<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    slice_qp_y: i32,
    slice_params: &SliceParams,
    x0: u32,
    y0: u32,
    log2_cb_size: u8,
    cb_depth: u8,
) -> Result<(), DecodeError> {
    let cu_transquant_bypass = if pps.transquant_bypass_enabled_flag {
        cabac.decode_bin(&mut contexts.state[ctx::CU_TRANSQUANT_BYPASS_FLAG]) != 0
    } else {
        false
    };
    if cu_transquant_bypass {
        mark_no_filter(state, x0, y0, log2_cb_size);
    }

    let cb_size = 1u32 << log2_cb_size;
    let x_cb = (x0 >> state.log2_min_cb_size) as usize;
    let y_cb = (y0 >> state.log2_min_cb_size) as usize;
    let length = (cb_size >> state.log2_min_cb_size) as usize;

    // ---- Step 1: cu_skip_flag (P/B slices only) ----
    let mut pred_mode = PredMode::Intra;
    if slice_params.slice_type != SliceType::I {
        let skip_flag = decode_skip_flag(cabac, contexts, state, x0, y0);
        // Write skip_flag for neighbor context derivation.
        for j in 0..length {
            let row = (y_cb + j) * state.min_cb_width;
            for i in 0..length {
                state.tab_skip_flag[row + x_cb + i] = skip_flag as u8;
            }
        }
        pred_mode = if skip_flag != 0 {
            PredMode::Skip
        } else {
            PredMode::Inter
        };
    } else {
        // I-slice: zero the skip flag for this CU.
        for j in 0..length {
            let row = (y_cb + j) * state.min_cb_width;
            for i in 0..length {
                state.tab_skip_flag[row + x_cb + i] = 0;
            }
        }
    }

    // ---- Step 2: skip CU path (merge only, no residual) ----
    if pred_mode == PredMode::Skip {
        decode_prediction_unit(
            cabac,
            contexts,
            state,
            slice_params,
            x0,
            y0,
            cb_size,
            cb_size,
            log2_cb_size,
            true,
            cb_depth,
            PartMode::Part2Nx2N,
            0,
        )?;
        // Write default intra pred modes (DC) for the skip CU so that
        // subsequent intra CUs' MPM derivation sees valid modes.
        write_intra_pred_mode(state, x0, y0, cb_size, INTRA_DC);
        // Phase 3d-6: actual motion compensation (replaces 128-fill placeholder).
        inter_pred::motion_compensation_cu(
            state,
            &slice_params.ref_frames_l0,
            &slice_params.ref_frames_l1,
            x0,
            y0,
            cb_size,
            PartMode::Part2Nx2N,
            slice_params.weighted_pred_flag,
            &slice_params.pred_weight_table,
        );
        // Skip CUs have no residual and no cu_qp_delta. Mirror FFmpeg
        // hls_coding_unit:2588-2595: if the QP group's delta hasn't been
        // decoded yet, still invoke `set_qPy` with delta=0 so `last_qp_y`
        // reflects the spatial prediction (qpy_a + qpy_b + 1) >> 1. Then
        // record the resulting QP in `tab_qp_y` for deblocking.
        if pps.cu_qp_delta_enabled_flag && !state.is_cu_qp_delta_coded {
            set_qpy(state, sps, pps, slice_qp_y, x0, y0);
        }
        write_qp_y_table(state, x0, y0, log2_cb_size, state.last_qp_y);
        maybe_save_qpy_pred(state, sps, pps, x0, y0, log2_cb_size);
        // Deblocking for inter skip/no-residual: compute bS per spec 8.7.2.
        compute_deblocking_boundary_strengths(state, slice_params, x0, y0, log2_cb_size, true);
        state.cu_count += 1;
        return Ok(());
    }

    // ---- Step 3: non-skip path ----
    // pred_mode_flag: for P/B slices, decode it. 1 = intra, 0 = inter.
    if slice_params.slice_type != SliceType::I {
        let pm_flag = cabac.decode_bin(&mut contexts.state[ctx::PRED_MODE_FLAG]);
        pred_mode = if pm_flag != 0 {
            PredMode::Intra
        } else {
            PredMode::Inter
        };
    }

    // ---- Step 4: part_mode ----
    let part_mode = if pred_mode != PredMode::Intra || log2_cb_size == state.log2_min_cb_size {
        decode_part_mode(cabac, contexts, sps, log2_cb_size, pred_mode)
    } else {
        PartMode::Part2Nx2N
    };

    let intra_split = part_mode == PartMode::PartNxN && pred_mode == PredMode::Intra;

    // ---- Step 5: intra or inter prediction ----
    let mut merge_flag_for_rqt = false;
    if pred_mode == PredMode::Intra {
        // PCM decode gate.
        let pcm_allowed = sps.pcm_enabled_flag
            && part_mode == PartMode::Part2Nx2N
            && log2_cb_size >= sps.log2_min_pcm_cb_size
            && log2_cb_size <= sps.log2_max_pcm_cb_size;
        if pcm_allowed && cabac.decode_terminate() != 0 {
            decode_pcm_block(cabac, state, sps, x0, y0, log2_cb_size)?;
            if sps.pcm_loop_filter_disabled_flag {
                mark_no_filter(state, x0, y0, log2_cb_size);
            }
            // A PCM CU has no transform tree, but its edges still need
            // boundary strengths (intra: bS = 2) and its QP must still be
            // recorded for deblocking and QP prediction.
            compute_deblocking_boundary_strengths(state, slice_params, x0, y0, log2_cb_size, true);
            finish_coding_unit(state, sps, pps, slice_qp_y, x0, y0, log2_cb_size);
            return Ok(());
        }
        decode_intra_mode_signaling(cabac, contexts, state, x0, y0, log2_cb_size, part_mode)?;
    } else {
        // Inter prediction: write default intra modes (DC) for MPM derivation.
        write_intra_pred_mode(state, x0, y0, cb_size, INTRA_DC);

        // Decode prediction units based on part_mode.
        match part_mode {
            PartMode::Part2Nx2N => {
                merge_flag_for_rqt = decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0,
                    cb_size,
                    cb_size,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    0,
                )?;
            }
            PartMode::Part2NxN => {
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0,
                    cb_size,
                    cb_size / 2,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    0,
                )?;
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0 + cb_size / 2,
                    cb_size,
                    cb_size / 2,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    1,
                )?;
            }
            PartMode::PartNx2N => {
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0,
                    cb_size / 2,
                    cb_size,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    0,
                )?;
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0 + cb_size / 2,
                    y0,
                    cb_size / 2,
                    cb_size,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    1,
                )?;
            }
            PartMode::Part2NxnU => {
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0,
                    cb_size,
                    cb_size / 4,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    0,
                )?;
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0 + cb_size / 4,
                    cb_size,
                    cb_size * 3 / 4,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    1,
                )?;
            }
            PartMode::Part2NxnD => {
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0,
                    cb_size,
                    cb_size * 3 / 4,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    0,
                )?;
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0 + cb_size * 3 / 4,
                    cb_size,
                    cb_size / 4,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    1,
                )?;
            }
            PartMode::PartnLx2N => {
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0,
                    cb_size / 4,
                    cb_size,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    0,
                )?;
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0 + cb_size / 4,
                    y0,
                    cb_size * 3 / 4,
                    cb_size,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    1,
                )?;
            }
            PartMode::PartnRx2N => {
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0,
                    y0,
                    cb_size * 3 / 4,
                    cb_size,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    0,
                )?;
                decode_prediction_unit(
                    cabac,
                    contexts,
                    state,
                    slice_params,
                    x0 + cb_size * 3 / 4,
                    y0,
                    cb_size / 4,
                    cb_size,
                    log2_cb_size,
                    false,
                    cb_depth,
                    part_mode,
                    1,
                )?;
            }
            PartMode::PartNxN => {
                // NxN for inter: 4 sub-PUs, each cb_size/2 x cb_size/2.
                let half = cb_size / 2;
                let mut pidx = 0u8;
                for pi in 0..2u32 {
                    for pj in 0..2u32 {
                        decode_prediction_unit(
                            cabac,
                            contexts,
                            state,
                            slice_params,
                            x0 + pj * half,
                            y0 + pi * half,
                            half,
                            half,
                            log2_cb_size,
                            false,
                            cb_depth,
                            part_mode,
                            pidx,
                        )?;
                        pidx += 1;
                    }
                }
            }
        }

        // Phase 3d-6: actual motion compensation (replaces 128-fill placeholder).
        inter_pred::motion_compensation_cu(
            state,
            &slice_params.ref_frames_l0,
            &slice_params.ref_frames_l1,
            x0,
            y0,
            cb_size,
            part_mode,
            slice_params.weighted_pred_flag,
            &slice_params.pred_weight_table,
        );
    }

    // ---- Step 6: transform tree / residual ----
    {
        let rqt_root_cbf = if pred_mode != PredMode::Intra
            && !(part_mode == PartMode::Part2Nx2N && merge_flag_for_rqt)
        {
            cabac.decode_bin(&mut contexts.state[ctx::NO_RESIDUAL_DATA_FLAG]) != 0
        } else {
            pred_mode == PredMode::Intra || (part_mode == PartMode::Part2Nx2N && merge_flag_for_rqt)
        };
        if rqt_root_cbf {
            let max_trafo_depth = if pred_mode == PredMode::Intra {
                sps.max_transform_hierarchy_depth_intra + if intra_split { 1 } else { 0 }
            } else {
                sps.max_transform_hierarchy_depth_inter
            };
            // Inter CUs with non-2Nx2N partition and
            // max_transform_hierarchy_depth_inter == 0 force an implicit TU
            // split at trafo_depth 0 (FFmpeg `inter_split` variable).
            let inter_split = sps.max_transform_hierarchy_depth_inter == 0
                && pred_mode != PredMode::Intra
                && part_mode != PartMode::Part2Nx2N;

            state.cu_origin = (x0, y0);
            state.cu_single_pu = part_mode == PartMode::Part2Nx2N;
            decode_transform_tree(
                cabac,
                contexts,
                state,
                sps,
                pps,
                slice_qp_y,
                pred_mode,
                x0,
                y0,
                x0,
                y0,
                log2_cb_size,
                log2_cb_size,
                0,
                max_trafo_depth,
                intra_split,
                inter_split,
                0,
                TransformTreeCbf::default(),
                slice_params,
                cu_transquant_bypass,
            )?;
        } else {
            // No residual: the end-of-CU block below still runs the
            // fallback set_qPy and writes tab_qp_y. We only need to
            // compute boundary strengths here (for inter no-residual).
            let single_pu = part_mode == PartMode::Part2Nx2N;
            compute_deblocking_boundary_strengths(
                state,
                slice_params,
                x0,
                y0,
                log2_cb_size,
                single_pu,
            );
        }
    }

    finish_coding_unit(state, sps, pps, slice_qp_y, x0, y0, log2_cb_size);
    Ok(())
}

/// End-of-CU bookkeeping shared by every CU, PCM included.
#[allow(clippy::too_many_arguments)]
fn finish_coding_unit<P: Pixel>(
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    slice_qp_y: i32,
    x0: u32,
    y0: u32,
    log2_cb_size: u8,
) {
    // Spec 8.6.1 / FFmpeg hls_coding_unit:2588-2589: at the end of every
    // CU, if `cu_qp_delta` has not yet been coded for the current QP
    // group, invoke `set_qPy` with delta=0 to refresh `last_qp_y` from
    // the spatial prediction. QP groups whose residuals happen to be
    // entirely zero still propagate the correct QP forward through the
    // `qpy_pred` chain.
    if pps.cu_qp_delta_enabled_flag && !state.is_cu_qp_delta_coded {
        set_qpy(state, sps, pps, slice_qp_y, x0, y0);
    }
    // FFmpeg hls_coding_unit:2591-2595: stamp the CU's QP into
    // `tab_qp_y` for all min-CBs the CU covers. Happens AFTER the
    // fallback set_qPy so `last_qp_y` reflects the prediction.
    write_qp_y_table(state, x0, y0, log2_cb_size, state.last_qp_y);

    // Spec 8.6.1 / FFmpeg hls_coding_unit:2597-2600: at the bottom-right
    // boundary of a QP group, snapshot `last_qp_y` into `qpy_pred` so the
    // next group's `get_qPy_pred` can fall back to it.
    maybe_save_qpy_pred(state, sps, pps, x0, y0, log2_cb_size);

    state.cu_count += 1;
}

/// Mark a CU's 4x4 blocks as protected from deblocking and SAO.
fn mark_no_filter<P: Pixel>(state: &mut PictureState<P>, x0: u32, y0: u32, log2_size: u8) {
    let n = ((1usize << log2_size) >> 2).max(1);
    let (bx, by) = ((x0 >> 2) as usize, (y0 >> 2) as usize);
    for j in 0..n {
        for i in 0..n {
            let idx = (by + j) * state.min_pu_width + bx + i;
            if let Some(v) = state.tab_no_filter.get_mut(idx) {
                *v = 1;
            }
        }
    }
    state.any_no_filter = true;
}

/// Decode a PCM (raw pixel) CU (HEVC spec 7.3.8.6 / FFmpeg `hls_pcm_sample`).
///
/// The PCM sample payload is bit-packed in the order Y, then Cb, then Cr.
/// Total length in bits is
///
/// ```text
///     cb_size * cb_size * pcm_bit_depth
///   + 2 * (cb_size/SubWidthC) * (cb_size/SubHeightC) * pcm_bit_depth_chroma
/// ```
///
/// and the CABAC engine is reinitialized at the next byte boundary afterwards.
///
/// Samples are stored scaled by `1 << (BitDepth - PcmBitDepth)` — i.e. the
/// reconstructed picture's bit depth may be larger than the PCM sample bit
/// depth, in which case PCM samples get left-shifted to match. We only
/// support 8-bit reconstruction today so the shift is in [0, 7].
fn decode_pcm_block<P: Pixel>(
    cabac: &mut CabacReader,
    state: &mut PictureState<P>,
    sps: &Sps,
    x0: u32,
    y0: u32,
    log2_cb_size: u8,
) -> Result<(), DecodeError> {
    let cb_size = 1usize << log2_cb_size;
    let pcm_bd_luma: u8 = sps.pcm_sample_bit_depth_luma;
    let pcm_bd_chroma: u8 = sps.pcm_sample_bit_depth_chroma;
    // `pcm_bit_depth` is guaranteed <= `bit_depth_luma/chroma` by sps.rs, so
    // these shifts are in [0, 7] for 8-bit reconstruction.
    let luma_shift: u8 = sps.bit_depth_luma - pcm_bd_luma;
    let chroma_shift: u8 = sps.bit_depth_chroma - pcm_bd_chroma;

    // Byte offset where the raw PCM bytes live.
    let pcm_start = cabac.pcm_byte_position();

    // Total payload length in bits (spec 7.3.8.6). The chroma block is
    // `cb_size / SubWidthC` × `cb_size / SubHeightC`: half-size for 4:2:0,
    // full-size for 4:4:4.
    let cb_chroma_w = cb_size >> state.chroma_shift_w;
    let cb_chroma_h = cb_size >> state.chroma_shift_h;
    let length_bits = cb_size * cb_size * pcm_bd_luma as usize
        + 2 * cb_chroma_w * cb_chroma_h * pcm_bd_chroma as usize;
    let length_bytes = length_bits.div_ceil(8);

    // Bounds check against the RBSP buffer.
    if pcm_start + length_bytes > cabac.rbsp().len() {
        return Err(DecodeError::UnexpectedEof);
    }
    // Borrow once so we don't alias `cabac` across the read loop.
    let pcm_bytes: Vec<u8> = cabac.rbsp()[pcm_start..pcm_start + length_bytes].to_vec();

    let mut reader = PcmBitReader::new(&pcm_bytes);

    // Luma plane write.
    {
        let stride = state.y_stride;
        let dst_off = (y0 as usize) * stride + (x0 as usize);
        for j in 0..cb_size {
            for i in 0..cb_size {
                let sample = reader.read_bits(pcm_bd_luma);
                state.y_plane[dst_off + j * stride + i] =
                    P::from_i32_clamped((sample << luma_shift) as i32, state.bit_depth);
            }
        }
    }

    // Chroma planes (Cb, Cr). Both planes are `cb_chroma_w` × `cb_chroma_h`
    // and share the same bit depth.
    {
        let stride = state.uv_stride;
        let x_c = (x0 as usize) >> state.chroma_shift_w;
        let y_c = (y0 as usize) >> state.chroma_shift_h;
        let dst_off = y_c * stride + x_c;
        for plane_idx in 0..2 {
            let plane = if plane_idx == 0 {
                &mut state.u_plane
            } else {
                &mut state.v_plane
            };
            for j in 0..cb_chroma_h {
                for i in 0..cb_chroma_w {
                    let sample = reader.read_bits(pcm_bd_chroma);
                    plane[dst_off + j * stride + i] =
                        P::from_i32_clamped((sample << chroma_shift) as i32, state.bit_depth_c);
                }
            }
        }
    }

    // Record the luma mode for subsequent CUs' MPM derivation. PCM CUs
    // contribute `INTRA_DC` to the IPM table (spec 8.4.2, same as
    // "not available" — handled by `intra_prediction_unit_default_value`
    // in FFmpeg).
    let pb_size = cb_size as u32;
    write_intra_pred_mode(state, x0, y0, pb_size, INTRA_DC);
    state.last_luma_pred_mode = INTRA_DC;
    state.last_chroma_pred_mode = INTRA_DC;

    // Reinit CABAC at the byte boundary immediately after the PCM payload.
    cabac.reinit_at(pcm_start + length_bytes)?;

    Ok(())
}

/// Minimal bit-packed reader for PCM samples (MSB-first within each byte).
struct PcmBitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> PcmBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn read_bits(&mut self, n: u8) -> u32 {
        // Fast path for 8-bit byte-aligned reads (the common case).
        if n == 8 && self.bit_pos.is_multiple_of(8) {
            let b = self.data[self.bit_pos / 8] as u32;
            self.bit_pos += 8;
            return b;
        }
        let mut val: u32 = 0;
        for _ in 0..n {
            let byte = self.data[self.bit_pos / 8] as u32;
            let bit = (byte >> (7 - (self.bit_pos & 7))) & 1;
            val = (val << 1) | bit;
            self.bit_pos += 1;
        }
        val
    }
}

/// Inherited cbf state passed down through `decode_transform_tree` recursion.
/// Once a parent has `cbf_cb = 0`, the child does not re-decode it (spec
/// 7.3.8.10 / FFmpeg `cbf_cb[]` propagation).
#[derive(Debug, Clone, Copy, Default)]
struct TransformTreeCbf {
    cbf_cb: bool,
    cbf_cr: bool,
}

/// Recursive transform tree decode (HEVC spec 7.3.8.10).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::only_used_in_recursion)]
fn decode_transform_tree<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    slice_qp_y: i32,
    pred_mode: PredMode,
    x0: u32,
    y0: u32,
    x_base: u32,
    y_base: u32,
    log2_cb_size: u8,
    log2_trafo_size: u8,
    trafo_depth: u8,
    max_trafo_depth: u32,
    intra_split: bool,
    inter_split: bool,
    blk_idx: u8,
    parent_cbf: TransformTreeCbf,
    slice_params: &SliceParams,
    cu_transquant_bypass: bool,
) -> Result<TransformTreeCbf, DecodeError> {
    // 1) Decide split_transform_flag (FFmpeg `hls_transform_tree` lines 1566-1580).
    let split_transform_flag = if log2_trafo_size <= sps.max_tb_log2_size_y
        && log2_trafo_size > sps.min_tb_log2_size_y
        && (trafo_depth as u32) < max_trafo_depth
        && !(intra_split && trafo_depth == 0)
    {
        decode_split_transform_flag(cabac, contexts, log2_trafo_size) != 0
    } else {
        // Implicit split: oversized TU, intra_split forcing depth-1, or inter
        // split (max_transform_hierarchy_depth_inter==0 + non-2Nx2N partition).
        log2_trafo_size > sps.max_tb_log2_size_y || (intra_split && trafo_depth == 0) || inter_split
    };

    state.last_split_transform_flag = split_transform_flag;

    // 2) Chroma cbf decode (spec 7.3.8.8). Signalled when the chroma TB would
    //    be at least 4×4: `log2_trafo_size > 2`, or always for 4:4:4
    //    (`ChromaArrayType == 3`), where the chroma TB matches the luma TB and
    //    can be 4×4. (4:2:0's 2×2 chroma TB is illegal, hence the size gate.)
    //    The flag is decoded when either (a) we're at the transform-tree root,
    //    or (b) the parent already had a non-zero cbf.
    let mut cbf_cb = parent_cbf.cbf_cb;
    let mut cbf_cr = parent_cbf.cbf_cr;
    let cat = sps.chroma_array_type();
    if cat != 0 && (log2_trafo_size > 2 || cat == 3) {
        if trafo_depth == 0 || parent_cbf.cbf_cb {
            cbf_cb = decode_cbf_cb_cr(cabac, contexts, trafo_depth) != 0;
        }
        if trafo_depth == 0 || parent_cbf.cbf_cr {
            cbf_cr = decode_cbf_cb_cr(cabac, contexts, trafo_depth) != 0;
        }
    }
    state.last_cbf_cb = cbf_cb;
    state.last_cbf_cr = cbf_cr;

    let inherited = TransformTreeCbf { cbf_cb, cbf_cr };

    if split_transform_flag {
        let trafo_size_split = 1u32 << (log2_trafo_size - 1);
        let x1 = x0 + trafo_size_split;
        let y1 = y0 + trafo_size_split;
        // All four sub-trees see the SAME parent cbf (the values decoded at
        // THIS level), not their sibling's decoded cbf. FFmpeg
        // `hls_transform_tree:1613-1616` SUBDIVIDE macro passes the same
        // `cbf_cb, cbf_cr` arrays to each sibling. Using a sibling's return
        // as the next sibling's parent_cbf causes `cbf_cr` to be skipped
        // whenever the first sibling's subtree decodes cbf_cr=0.
        let _ = decode_transform_tree(
            cabac,
            contexts,
            state,
            sps,
            pps,
            slice_qp_y,
            pred_mode,
            x0,
            y0,
            x0,
            y0,
            log2_cb_size,
            log2_trafo_size - 1,
            trafo_depth + 1,
            max_trafo_depth,
            intra_split,
            false, // inter_split only at depth 0
            0,
            inherited,
            slice_params,
            cu_transquant_bypass,
        )?;
        let _ = decode_transform_tree(
            cabac,
            contexts,
            state,
            sps,
            pps,
            slice_qp_y,
            pred_mode,
            x1,
            y0,
            x0,
            y0,
            log2_cb_size,
            log2_trafo_size - 1,
            trafo_depth + 1,
            max_trafo_depth,
            intra_split,
            false,
            1,
            inherited,
            slice_params,
            cu_transquant_bypass,
        )?;
        let _ = decode_transform_tree(
            cabac,
            contexts,
            state,
            sps,
            pps,
            slice_qp_y,
            pred_mode,
            x0,
            y1,
            x0,
            y0,
            log2_cb_size,
            log2_trafo_size - 1,
            trafo_depth + 1,
            max_trafo_depth,
            intra_split,
            false,
            2,
            inherited,
            slice_params,
            cu_transquant_bypass,
        )?;
        let final_cbf = decode_transform_tree(
            cabac,
            contexts,
            state,
            sps,
            pps,
            slice_qp_y,
            pred_mode,
            x1,
            y1,
            x0,
            y0,
            log2_cb_size,
            log2_trafo_size - 1,
            trafo_depth + 1,
            max_trafo_depth,
            intra_split,
            false,
            3,
            inherited,
            slice_params,
            cu_transquant_bypass,
        )?;
        Ok(final_cbf)
    } else {
        decode_transform_unit(
            cabac,
            contexts,
            state,
            sps,
            pps,
            slice_qp_y,
            pred_mode,
            x0,
            y0,
            x_base,
            y_base,
            log2_trafo_size,
            trafo_depth,
            blk_idx,
            inherited,
            slice_params,
            cu_transquant_bypass,
        )
    }
}

/// Decode `split_transform_flag` (HEVC spec 9.3.4.2.5).
/// Context offset: `SPLIT_TRANSFORM_FLAG + (5 - log2_trafo_size)`.
fn decode_split_transform_flag(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    log2_trafo_size: u8,
) -> u32 {
    let inc = 5usize - log2_trafo_size as usize;
    cabac.decode_bin(&mut contexts.state[ctx::SPLIT_TRANSFORM_FLAG + inc])
}

/// Decode `cbf_cb` or `cbf_cr` — same context structure (HEVC spec 9.3.4.2.6).
/// Context offset: `CBF_CB_CR + trafo_depth`.
fn decode_cbf_cb_cr(cabac: &mut CabacReader, contexts: &mut CabacContexts, trafo_depth: u8) -> u32 {
    cabac.decode_bin(&mut contexts.state[ctx::CBF_CB_CR + trafo_depth as usize])
}

/// Decode `cbf_luma` (HEVC spec 9.3.4.2.6).
/// Context offset: `CBF_LUMA + (trafo_depth == 0 ? 1 : 0)`.
/// (FFmpeg expresses this as `CBF_LUMA + !trafo_depth`.)
fn decode_cbf_luma(cabac: &mut CabacReader, contexts: &mut CabacContexts, trafo_depth: u8) -> u32 {
    let inc = if trafo_depth == 0 { 1 } else { 0 };
    cabac.decode_bin(&mut contexts.state[ctx::CBF_LUMA + inc])
}

/// Decode `cu_qp_delta_abs` (HEVC spec 9.3.4.2.7) — truncated unary prefix
/// (max value 5) followed by an Exp-Golomb-0 suffix when the prefix is at
/// its max.
fn decode_cu_qp_delta_abs(cabac: &mut CabacReader, contexts: &mut CabacContexts) -> u32 {
    let mut prefix = 0u32;
    let mut inc = 0usize;
    while prefix < 5 && cabac.decode_bin(&mut contexts.state[ctx::CU_QP_DELTA + inc]) != 0 {
        prefix += 1;
        inc = 1;
    }
    if prefix < 5 {
        return prefix;
    }
    // EG-0 suffix: read bypass bits until a 0, then `k` more.
    let mut suffix = 0u32;
    let mut k = 0u32;
    while k < 7 && cabac.decode_bypass() != 0 {
        suffix += 1 << k;
        k += 1;
    }
    while k > 0 {
        k -= 1;
        suffix += cabac.decode_bypass() << k;
    }
    prefix + suffix
}

/// Decode `cu_qp_delta_sign_flag` (bypass).
fn decode_cu_qp_delta_sign_flag(cabac: &mut CabacReader) -> u32 {
    cabac.decode_bypass()
}

/// `transform_unit` decode (spec 7.3.8.11). Decodes the cbf flags,
/// `cu_qp_delta`, and (if any cbf is set) the per-plane residual_coding.
/// For intra CUs: performs intra prediction and reconstruction.
/// For inter CUs: prediction was already written as placeholder (128)
/// and residual is added on top.
#[allow(clippy::too_many_arguments)]
fn decode_transform_unit<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    slice_qp_y: i32,
    pred_mode: PredMode,
    x0: u32,
    y0: u32,
    x_base: u32,
    y_base: u32,
    log2_trafo_size: u8,
    trafo_depth: u8,
    blk_idx: u8,
    inherited: TransformTreeCbf,
    slice_params: &SliceParams,
    cu_transquant_bypass: bool,
) -> Result<TransformTreeCbf, DecodeError> {
    let is_intra = pred_mode == PredMode::Intra;

    // ---- Step 1: luma intra prediction (only for intra CUs).
    if is_intra {
        // For PART_NxN (intra_split), each sub-CU has its own mode stored
        // in tab_ipm. Look it up from the TU position rather than using the
        // CU-level `last_luma_pred_mode` which only holds mode[0].
        let x_pu = (x0 >> state.log2_min_pu_size) as usize;
        let y_pu = (y0 >> state.log2_min_pu_size) as usize;
        let luma_mode = state.tab_ipm[y_pu * state.min_pu_width + x_pu];
        predict_intra_luma(
            state,
            sps,
            x0,
            y0,
            log2_trafo_size,
            luma_mode,
            pps.constrained_intra_pred_flag,
            cu_transquant_bypass,
        )?;
    }

    // ---- Step 2: cbf_luma decode.
    // FFmpeg gates cbf_luma decoding behind:
    //   pred_mode == INTRA || trafo_depth != 0 || any chroma cbf set
    let cbf_luma = if is_intra || trafo_depth != 0 || inherited.cbf_cb || inherited.cbf_cr {
        decode_cbf_luma(cabac, contexts, trafo_depth) != 0
    } else {
        true
    };
    state.last_cbf_luma = cbf_luma;

    // Write cbf_luma to per-TU table for deblocking boundary strength.
    {
        let tu_size = 1u32 << log2_trafo_size;
        let x_tu_start = (x0 >> state.log2_min_tb_size) as usize;
        let y_tu_start = (y0 >> state.log2_min_tb_size) as usize;
        let tu_count = (tu_size >> state.log2_min_tb_size).max(1) as usize;
        let val = if cbf_luma { 1u8 } else { 0u8 };
        for j in 0..tu_count {
            for i in 0..tu_count {
                let idx = (y_tu_start + j) * state.min_tb_width + (x_tu_start + i);
                if idx < state.tab_cbf_luma.len() {
                    state.tab_cbf_luma[idx] = val;
                }
            }
        }
    }

    let new_cbf = inherited;
    let do_chroma_inline = sps.chroma_format_idc == 1 && log2_trafo_size > 2;
    let do_chroma_deferred = sps.chroma_format_idc == 1 && log2_trafo_size == 2 && blk_idx == 3;
    // 4:4:4: the chroma TB matches the luma TB one-for-one — processed at every
    // leaf, at the luma position and size, with no half-size scaling and no
    // 4×4 deferral.
    let do_chroma_444 = sps.chroma_array_type() == 3;

    if cbf_luma || inherited.cbf_cb || inherited.cbf_cr {
        if pps.cu_qp_delta_enabled_flag && !state.is_cu_qp_delta_coded {
            let abs = decode_cu_qp_delta_abs(cabac, contexts) as i32;
            let signed = if abs != 0 {
                let sign = decode_cu_qp_delta_sign_flag(cabac);
                if sign != 0 { -abs } else { abs }
            } else {
                0
            };
            state.last_cu_qp_delta = signed;
            // CuQpDeltaVal range: -(26 + QpBdOffsetY/2) ..= 25 + QpBdOffsetY/2.
            let half_bd = 3 * (sps.bit_depth_luma as i32 - 8);
            if !(-(26 + half_bd)..=25 + half_bd).contains(&signed) {
                return Err(DecodeError::InvalidSyntax("cu_qp_delta out of range"));
            }
            state.is_cu_qp_delta_coded = true;
            // Spec 8.6.1: apply the delta to the prediction derived at the
            // CU origin (xCb, yCb), as FFmpeg hls_transform_unit passes
            // cb_xBase / cb_yBase. A TU origin is not equivalent: in a CU
            // larger than the QP group, a TU away from the CU origin falls
            // in a different group and would read neighbour QPs from inside
            // its own, not yet decoded, CU.
            let (cu_x, cu_y) = state.cu_origin;
            set_qpy(state, sps, pps, slice_qp_y, cu_x, cu_y);
        }

        // cu_chroma_qp_offset (spec 7.3.8.11 / FFmpeg hevcdec.c:1349-1366).
        // Decoded once per CU on the first TU with non-zero chroma cbf.
        let cbf_chroma = inherited.cbf_cb || inherited.cbf_cr;
        if slice_params.cu_chroma_qp_offset_enabled_flag
            && cbf_chroma
            && !cu_transquant_bypass
            && !state.is_cu_chroma_qp_offset_coded
        {
            let cu_chroma_qp_offset_flag =
                cabac.decode_bin(&mut contexts.state[ctx::CU_CHROMA_QP_OFFSET_FLAG]);
            if cu_chroma_qp_offset_flag != 0 {
                let mut cu_chroma_qp_offset_idx = 0u32;
                if pps.chroma_qp_offset_list_len_minus1 > 0 {
                    // Truncated unary binarization, max = chroma_qp_offset_list_len_minus1.
                    cu_chroma_qp_offset_idx = 0;
                    while cu_chroma_qp_offset_idx < pps.chroma_qp_offset_list_len_minus1 {
                        let bin =
                            cabac.decode_bin(&mut contexts.state[ctx::CU_CHROMA_QP_OFFSET_IDX]);
                        if bin == 0 {
                            break;
                        }
                        cu_chroma_qp_offset_idx += 1;
                    }
                }
                state.cu_qp_offset_cb = pps.cb_qp_offset_list[cu_chroma_qp_offset_idx as usize];
                state.cu_qp_offset_cr = pps.cr_qp_offset_list[cu_chroma_qp_offset_idx as usize];
            } else {
                state.cu_qp_offset_cb = 0;
                state.cu_qp_offset_cr = 0;
            }
            state.is_cu_chroma_qp_offset_coded = true;
        }

        let qp_y = state.last_qp_y;
        // Cross-component prediction (spec 7.3.8.12, 4:4:4 only): needs a
        // coded luma residual in this transform unit.
        let ccp_possible = pps.cross_component_prediction_enabled_flag && cbf_luma && do_chroma_444;

        // ---- Step 3: luma residual_coding + IDCT + reconstruction.
        if cbf_luma {
            let luma_mode = if is_intra {
                let x_pu = (x0 >> state.log2_min_pu_size) as usize;
                let y_pu = (y0 >> state.log2_min_pu_size) as usize;
                state.tab_ipm[y_pu * state.min_pu_width + x_pu]
            } else {
                0
            };
            let scan_idx = if is_intra {
                pick_scan_order(log2_trafo_size, luma_mode)
            } else {
                ScanOrder::Diag
            };
            let mut block = decode_residual_coding(
                cabac,
                contexts,
                sps,
                pps,
                log2_trafo_size,
                ResidualPlane::Luma,
                qp_y,
                scan_idx,
                is_intra,
                luma_mode,
                cu_transquant_bypass,
                std::mem::take(&mut state.coeff_buf),
            )?;
            // Tests inspect the dequantized coefficients of the last block.
            #[cfg(test)]
            {
                state.last_luma_residual = Some(block.clone());
            }
            apply_residual_to_luma(
                state,
                x0,
                y0,
                log2_trafo_size,
                &mut block,
                is_intra,
                pps.cross_component_prediction_enabled_flag,
            );
            state.coeff_buf = block.coeffs;
        }

        // ---- Step 4: chroma prediction + (optional) residual.
        // For 4:2:0, chroma TU is at log2_trafo_size - 1 (half the luma TU).
        // The chroma QP is derived from the luma QP via spec table 8-9.
        let log2_trafo_size_c = if do_chroma_444 {
            log2_trafo_size
        } else if do_chroma_inline {
            log2_trafo_size - 1
        } else if do_chroma_deferred {
            log2_trafo_size // deferred uses parent's log2 size
        } else {
            0 // unused
        };

        if is_intra {
            if do_chroma_444 {
                // 4:4:4 chroma modes are per-PU; look the leaf's up by
                // position and stage it so the residual scan order (which
                // reads `last_chroma_pred_mode`) matches.
                let x_pu = (x0 >> state.log2_min_pu_size) as usize;
                let y_pu = (y0 >> state.log2_min_pu_size) as usize;
                let chroma_mode = state.tab_chroma_ipm[y_pu * state.min_pu_width + x_pu];
                state.last_chroma_pred_mode = chroma_mode;
                // intra_chroma_pred_mode 4 (DM) is exactly when the derived
                // chroma mode equals the luma mode in 4:4:4.
                let ccp_intra =
                    ccp_possible && chroma_mode == state.tab_ipm[y_pu * state.min_pu_width + x_pu];
                predict_intra_chroma(
                    state,
                    sps,
                    x0,
                    y0,
                    log2_trafo_size,
                    chroma_mode,
                    pps.constrained_intra_pred_flag,
                )?;
                decode_chroma_residuals(
                    cabac,
                    contexts,
                    state,
                    sps,
                    pps,
                    x0,
                    y0,
                    log2_trafo_size_c,
                    log2_trafo_size,
                    qp_y,
                    inherited.cbf_cb,
                    inherited.cbf_cr,
                    true,
                    cu_transquant_bypass,
                    slice_params.slice_cb_qp_offset,
                    slice_params.slice_cr_qp_offset,
                    ccp_intra,
                )?;
            } else if do_chroma_inline {
                let chroma_mode = state.last_chroma_pred_mode;
                predict_intra_chroma(
                    state,
                    sps,
                    x0,
                    y0,
                    log2_trafo_size - 1,
                    chroma_mode,
                    pps.constrained_intra_pred_flag,
                )?;
                decode_chroma_residuals(
                    cabac,
                    contexts,
                    state,
                    sps,
                    pps,
                    x0,
                    y0,
                    log2_trafo_size_c,
                    log2_trafo_size,
                    qp_y,
                    inherited.cbf_cb,
                    inherited.cbf_cr,
                    true,
                    cu_transquant_bypass,
                    slice_params.slice_cb_qp_offset,
                    slice_params.slice_cr_qp_offset,
                    false,
                )?;
            } else if do_chroma_deferred {
                let chroma_mode = state.last_chroma_pred_mode;
                predict_intra_chroma(
                    state,
                    sps,
                    x_base,
                    y_base,
                    log2_trafo_size,
                    chroma_mode,
                    pps.constrained_intra_pred_flag,
                )?;
                decode_chroma_residuals(
                    cabac,
                    contexts,
                    state,
                    sps,
                    pps,
                    x_base,
                    y_base,
                    log2_trafo_size_c,
                    log2_trafo_size + 1,
                    qp_y,
                    inherited.cbf_cb,
                    inherited.cbf_cr,
                    true,
                    cu_transquant_bypass,
                    slice_params.slice_cb_qp_offset,
                    slice_params.slice_cr_qp_offset,
                    false,
                )?;
            }
        } else {
            // Inter chroma residual. Paths per spec 7.3.8.11:
            //  * `do_chroma_inline` (4:2:0) / `do_chroma_444` — chroma TU at
            //    the luma position; the size difference (half for 4:2:0, equal
            //    for 4:4:4) is already carried by `log2_trafo_size_c`, so the
            //    residual_coding() call is identical.
            //  * `do_chroma_deferred` — luma has split to 4×4 at blk_idx 3,
            //    so chroma residual lives at the parent's position and size.
            //    FFmpeg `hls_transform_unit` handles this unconditionally
            //    (whether intra or inter); our previous code only handled
            //    the intra side, so inter CUs with 4×4 luma TUs were
            //    silently skipping the chroma residual_coding() calls.
            // With cross-component prediction a chroma residual exists
            // even without a chroma cbf.
            let ccp_inter = ccp_possible;
            if (do_chroma_inline || do_chroma_444)
                && (inherited.cbf_cb || inherited.cbf_cr || ccp_inter)
            {
                decode_chroma_residuals(
                    cabac,
                    contexts,
                    state,
                    sps,
                    pps,
                    x0,
                    y0,
                    log2_trafo_size_c,
                    log2_trafo_size,
                    qp_y,
                    inherited.cbf_cb,
                    inherited.cbf_cr,
                    false,
                    cu_transquant_bypass,
                    slice_params.slice_cb_qp_offset,
                    slice_params.slice_cr_qp_offset,
                    ccp_inter,
                )?;
            } else if do_chroma_deferred && (inherited.cbf_cb || inherited.cbf_cr) {
                decode_chroma_residuals(
                    cabac,
                    contexts,
                    state,
                    sps,
                    pps,
                    x_base,
                    y_base,
                    log2_trafo_size_c,
                    log2_trafo_size + 1,
                    qp_y,
                    inherited.cbf_cb,
                    inherited.cbf_cr,
                    false,
                    cu_transquant_bypass,
                    slice_params.slice_cb_qp_offset,
                    slice_params.slice_cr_qp_offset,
                    false,
                )?;
            }
        }
    } else if is_intra {
        if do_chroma_444 {
            let x_pu = (x0 >> state.log2_min_pu_size) as usize;
            let y_pu = (y0 >> state.log2_min_pu_size) as usize;
            let chroma_mode = state.tab_chroma_ipm[y_pu * state.min_pu_width + x_pu];
            predict_intra_chroma(
                state,
                sps,
                x0,
                y0,
                log2_trafo_size,
                chroma_mode,
                pps.constrained_intra_pred_flag,
            )?;
        } else if do_chroma_inline {
            let chroma_mode = state.last_chroma_pred_mode;
            predict_intra_chroma(
                state,
                sps,
                x0,
                y0,
                log2_trafo_size - 1,
                chroma_mode,
                pps.constrained_intra_pred_flag,
            )?;
        } else if do_chroma_deferred {
            let chroma_mode = state.last_chroma_pred_mode;
            predict_intra_chroma(
                state,
                sps,
                x_base,
                y_base,
                log2_trafo_size,
                chroma_mode,
                pps.constrained_intra_pred_flag,
            )?;
        }
    }

    // ---- Step 5: deblocking bookkeeping.
    // NOTE: `tab_qp_y` is written at CU granularity in
    // `decode_coding_unit` (mirroring FFmpeg `hls_coding_unit:2591-2595`),
    // AFTER the fallback `set_qPy(delta=0)` has resolved a stable
    // `last_qp_y` for the whole CU. Writing per-TU here would stamp the
    // previous CU's QP into `tab_qp_y` for any TU that arrives before
    // `cu_qp_delta` is decoded.
    if is_intra {
        if !slice_params.deblocking_disabled {
            mark_intra_tu_boundaries(state, x0, y0, log2_trafo_size);
        }
    } else {
        // Inter TU leaf: compute boundary strengths using cbf_luma + MV comparison.
        let single_pu = state.cu_single_pu;
        compute_deblocking_boundary_strengths(
            state,
            slice_params,
            x0,
            y0,
            log2_trafo_size,
            single_pu,
        );
    }

    Ok(new_cbf)
}

/// HEVC spec 8.6.1 `get_qPy_pred`. Derives the predicted QP at the current
/// CU by averaging the QPs of the neighboring CU to the left and above
/// (taken at the origin of the enclosing QP group). If either neighbor is
/// unavailable (outside the CTB, or before the first QP-coded CU), the
/// saved `qpy_pred` from the previous group is used as fallback. Mirrors
/// FFmpeg `filter.c:get_qPy_pred`.
///
/// Side effect: updates `state.first_qp_group` to `!is_cu_qp_delta_coded`
/// when the function falls into the first-group / picture-origin branch,
/// matching FFmpeg's behavior so that multi-CU QP groups that never decode
/// a delta keep the flag set for the next group.
fn get_qpy_pred<P: Pixel>(
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    slice_qp: i32,
    x_base: u32,
    y_base: u32,
) -> i32 {
    let ctb_mask: u32 = (1u32 << sps.ctb_log2_size_y) - 1;
    let min_qp_log2 = sps
        .ctb_log2_size_y
        .saturating_sub(pps.diff_cu_qp_delta_depth as u8);
    let min_qp_mask: u32 = (1u32 << min_qp_log2) - 1;
    let x_qg = x_base & !min_qp_mask;
    let y_qg = y_base & !min_qp_mask;
    let x_cb = (x_qg >> state.log2_min_cb_size) as usize;
    let y_cb = (y_qg >> state.log2_min_cb_size) as usize;
    let available_a = x_cb > 0 && (x_base & ctb_mask) != 0 && (x_qg & ctb_mask) != 0;
    let available_b = y_cb > 0 && (y_base & ctb_mask) != 0 && (y_qg & ctb_mask) != 0;

    let qpy_pred_fallback = if state.first_qp_group || (x_qg == 0 && y_qg == 0) {
        state.first_qp_group = !state.is_cu_qp_delta_coded;
        slice_qp
    } else {
        state.qpy_pred
    };

    let qpy_a = if !available_a {
        qpy_pred_fallback
    } else {
        state.tab_qp_y[(x_cb - 1) + y_cb * state.min_cb_width] as i32
    };
    let qpy_b = if !available_b {
        qpy_pred_fallback
    } else {
        state.tab_qp_y[x_cb + (y_cb - 1) * state.min_cb_width] as i32
    };

    (qpy_a + qpy_b + 1) >> 1
}

/// HEVC spec 8.6.1 `set_qPy`. Computes the effective luma QP for a CU as
/// `qPy_pred + cu_qp_delta` with modular wrap into `[-qp_bd_offset, 51]`,
/// then stashes the result in `state.last_qp_y`. Mirrors FFmpeg
/// `filter.c:ff_hevc_set_qPy`; `qp_bd_offset = 6 * (BitDepthY - 8)`.
fn set_qpy<P: Pixel>(
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    slice_qp: i32,
    x_base: u32,
    y_base: u32,
) {
    let qp_pred = get_qpy_pred(state, sps, pps, slice_qp, x_base, y_base);
    let delta = state.last_cu_qp_delta;
    state.last_qp_y = if delta != 0 {
        // QpY = ((qPY_PRED + CuQpDeltaVal + 52 + 2*QpBdOffsetY) %
        //        (52 + QpBdOffsetY)) - QpBdOffsetY   (spec eq. 8-283)
        let off = 6 * (sps.bit_depth_luma as i32 - 8);
        (qp_pred + delta + 52 + 2 * off).rem_euclid(52 + off) - off
    } else {
        qp_pred
    };
}

/// End of a QP-group-aligned CU or split node: save `last_qp_y` as the
/// `qpy_pred` fallback for the next group (spec 8.6.1 / FFmpeg
/// hevcdec.c:2597-2600 and 2671-2673).
fn maybe_save_qpy_pred<P: Pixel>(
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    x0: u32,
    y0: u32,
    log2_cb_size: u8,
) {
    let min_qp_log2 = sps
        .ctb_log2_size_y
        .saturating_sub(pps.diff_cu_qp_delta_depth as u8);
    let mask: u32 = (1u32 << min_qp_log2) - 1;
    let cb_size = 1u32 << log2_cb_size;
    if ((x0 + cb_size) & mask) == 0 && ((y0 + cb_size) & mask) == 0 {
        state.qpy_pred = state.last_qp_y;
    }
}

/// Write `qp_y` into the per-min-CB QP table for all min-CB positions
/// covered by the TU at `(x0, y0)` of size `1 << log2_size`. Used by
/// the deblock pass to look up tc/β.
fn write_qp_y_table<P: Pixel>(
    state: &mut PictureState<P>,
    x0: u32,
    y0: u32,
    log2_size: u8,
    qp_y: i32,
) {
    let length = ((1u32 << log2_size) >> state.log2_min_cb_size).max(1) as usize;
    let x_cb = (x0 >> state.log2_min_cb_size) as usize;
    let y_cb = (y0 >> state.log2_min_cb_size) as usize;
    let qp_bd_offset = 6 * (state.bit_depth as i32 - 8);
    let v = qp_y.clamp(-qp_bd_offset, 51) as i8;
    for j in 0..length {
        let row = (y_cb + j) * state.min_cb_width;
        for i in 0..length {
            state.tab_qp_y[row + x_cb + i] = v;
        }
    }
}

/// Decode and apply chroma Cb/Cr residuals for a single TU. The chroma
/// QP is derived from the luma QP via the HEVC spec table 8-9 mapping.
/// `x0`/`y0` are in luma sample coordinates; chroma is at half that for 4:2:0.
/// `cross_comp_pred(x0, y0, c)` (spec 7.3.8.12): returns ResScaleVal,
/// `(1 << (log2_res_scale_abs_plus1 - 1)) * (1 - 2 * res_scale_sign_flag)`.
/// `log2_res_scale_abs_plus1` is truncated unary with cMax 4, context
/// `4 * c + binIdx`; the sign uses context `c`.
fn decode_cross_comp_pred(cabac: &mut CabacReader, contexts: &mut CabacContexts, c: usize) -> i32 {
    let mut abs_plus1 = 0usize;
    while abs_plus1 < 4
        && cabac.decode_bin(&mut contexts.state[ctx::LOG2_RES_SCALE_ABS + 4 * c + abs_plus1]) != 0
    {
        abs_plus1 += 1;
    }
    if abs_plus1 == 0 {
        return 0;
    }
    let sign = cabac.decode_bin(&mut contexts.state[ctx::RES_SCALE_SIGN_FLAG + c]) as i32;
    (1 << (abs_plus1 - 1)) * (1 - 2 * sign)
}

#[allow(clippy::too_many_arguments)]
fn decode_chroma_residuals<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    sps: &Sps,
    pps: &Pps,
    x0: u32,
    y0: u32,
    log2_trafo_size_c: u8,
    log2_trafo_size_luma: u8,
    qp_y: i32,
    cbf_cb: bool,
    cbf_cr: bool,
    is_intra: bool,
    cu_transquant_bypass: bool,
    slice_cb_qp_offset: i32,
    slice_cr_qp_offset: i32,
    ccp: bool,
) -> Result<(), DecodeError> {
    let x_c = (x0 >> state.chroma_shift_w) as usize;
    let y_c = (y0 >> state.chroma_shift_h) as usize;

    for c_idx in 1..=2u8 {
        let cbf = if c_idx == 1 { cbf_cb } else { cbf_cr };
        // cross_comp_pred(x0, y0, c) precedes the component's residual
        // (spec 7.3.8.12): ResScaleVal for this chroma component.
        let res_scale_val = if ccp {
            decode_cross_comp_pred(cabac, contexts, (c_idx - 1) as usize)
        } else {
            0
        };
        if !cbf && res_scale_val == 0 {
            continue;
        }
        // Derive chroma QP per spec 8.6.1.
        // Sum PPS-level, slice-level, and CU-level chroma QP offsets.
        let qp_offset = if c_idx == 1 {
            pps.pps_cb_qp_offset + slice_cb_qp_offset + state.cu_qp_offset_cb
        } else {
            pps.pps_cr_qp_offset + slice_cr_qp_offset + state.cu_qp_offset_cr
        };
        // qPiCb/Cr = Clip3(-QpBdOffsetC, 57, QpY + offsets) (spec 8.6.1).
        let qp_i = (qp_y + qp_offset).clamp(-6 * (sps.bit_depth_chroma as i32 - 8), 57);
        // ChromaArrayType 1 (4:2:0) maps qPi → QpC via table 8-10; 4:2:2 and
        // 4:4:4 use `Min(qPi, 51)` directly.
        let qp_c = if state.chroma_array_type == 1 {
            if qp_i < 30 {
                qp_i
            } else if qp_i > 43 {
                qp_i - 6
            } else {
                const QP_C: [i32; 14] = [29, 30, 31, 32, 33, 33, 34, 34, 35, 35, 36, 36, 37, 37];
                QP_C[(qp_i - 30) as usize]
            }
        } else {
            qp_i.min(51)
        };

        let plane = if c_idx == 1 {
            ResidualPlane::Cb
        } else {
            ResidualPlane::Cr
        };
        // Spec 7.4.9.11 / FFmpeg hevcdec.c:1377-1383: for intra TUs the
        // chroma scan_idx_c is selected based on chroma intra mode, but only
        // when the LUMA `log2_trafo_size < 4` (i.e. luma 4x4 or 8x8). For
        // larger luma TBs, chroma scan stays diagonal.
        let scan_idx = if is_intra && log2_trafo_size_luma < 4 {
            pick_scan_order(log2_trafo_size_c, state.last_chroma_pred_mode)
        } else {
            ScanOrder::Diag
        };
        let size_c = 1usize << log2_trafo_size_c;
        let mut residual = if cbf {
            let block = decode_residual_coding(
                cabac,
                contexts,
                sps,
                pps,
                log2_trafo_size_c,
                plane,
                qp_c,
                scan_idx,
                is_intra,
                state.last_chroma_pred_mode,
                cu_transquant_bypass,
                std::mem::take(&mut state.coeff_buf),
            )?;
            // Apply inverse transform + add to chroma plane.
            if let Some(wide) = &block.coeffs_wide {
                crate::inverse_transform::inverse_transform_ext(
                    wide,
                    log2_trafo_size_c,
                    sps.bit_depth_chroma as u32,
                    block.log2_range,
                    false,
                )
            } else {
                let mut residual = block.coeffs;
                if !block.transform_skip && !block.cu_transquant_bypass {
                    apply_inverse_transform(
                        &mut residual,
                        log2_trafo_size_c,
                        block.last_sig_x,
                        block.last_sig_y,
                        (block.nz_cols, block.nz_rows),
                        sps.bit_depth_chroma as u32,
                        false, // chroma never uses DST
                    );
                }
                residual
            }
        } else {
            let mut zeros = std::mem::take(&mut state.coeff_buf);
            zeros.clear();
            zeros.resize(size_c * size_c, 0);
            zeros
        };
        // Cross-component prediction (spec 8.6.6): add the scaled luma
        // residual. Only in 4:4:4, where the blocks are the same size.
        if res_scale_val != 0 && state.ccp_luma_residual.len() == residual.len() {
            let (bd_y, bd_c) = (sps.bit_depth_luma as i32, sps.bit_depth_chroma as i32);
            for (r, &y) in residual.iter_mut().zip(&state.ccp_luma_residual) {
                let scaled = (res_scale_val * (((y as i32) << bd_c) >> bd_y)) >> 3;
                *r = (*r as i32 + scaled) as i16;
            }
        }
        let dst_stride = state.uv_stride;
        let (bit_depth_c, row_exact) = (state.bit_depth_c, state.row_exact);
        let dst_plane = if c_idx == 1 {
            &mut state.u_plane
        } else {
            &mut state.v_plane
        };
        let dst_offset = y_c * dst_stride + x_c;
        with_block(
            dst_plane,
            &mut state.block_buf,
            row_exact,
            dst_offset,
            dst_stride,
            (size_c, size_c),
            |dst, stride| add_residual(dst, stride, &residual, log2_trafo_size_c, bit_depth_c),
        );
        state.coeff_buf = residual;
    }
    Ok(())
}

/// Mark the top and left edges of an intra TU at `(x0, y0)` of size
/// `1 << log2_size` with boundary strength 2 in the per-4×4 BS grid.
/// Skips picture borders.
fn mark_intra_tu_boundaries<P: Pixel>(
    state: &mut PictureState<P>,
    x0: u32,
    y0: u32,
    log2_size: u8,
) {
    let size = 1u32 << log2_size;
    let pic_w = state.width as usize;
    let bs_w = pic_w >> 2; // entries per row in the BS grid

    // Top edge: only mark if y0 > 0 (there's a TU above to deblock against).
    if y0 > 0 {
        let yy = (y0 >> 2) as usize;
        let xx_start = (x0 >> 2) as usize;
        let xx_end = ((x0 + size) >> 2) as usize;
        for xx in xx_start..xx_end {
            state.bs_horizontal[yy * bs_w + xx] = 2;
        }
    }
    // Left edge: only mark if x0 > 0.
    if x0 > 0 {
        let xx = (x0 >> 2) as usize;
        let yy_start = (y0 >> 2) as usize;
        let yy_end = ((y0 + size) >> 2) as usize;
        for yy in yy_start..yy_end {
            state.bs_vertical[yy * bs_w + xx] = 2;
        }
    }
}

/// Build the reference samples and call PLANAR/DC/angular for a luma TU.
/// Writes the prediction into `state.y_plane` at `(x0, y0)`.
#[allow(clippy::too_many_arguments)]
fn predict_intra_luma<P: Pixel>(
    state: &mut PictureState<P>,
    sps: &Sps,
    x0: u32,
    y0: u32,
    log2_size: u8,
    mode: u8,
    constrained_intra_pred_flag: bool,
    cu_transquant_bypass: bool,
) -> Result<(), DecodeError> {
    let size = 1usize << log2_size;
    let pic_w = state.width as usize;
    let pic_h = state.height as usize;
    let avail = compute_luma_avail_inner(state, x0, y0, size as u32, constrained_intra_pred_flag);
    // disableIntraBoundaryFilter (spec 8.4.4.2.6): the DC and horizontal /
    // vertical edge filters are off for lossless CUs using implicit RDPCM.
    let edge_filters = !(sps.range.implicit_rdpcm_enabled_flag && cu_transquant_bypass);
    let cip = constrained_intra_pred_flag
        .then(|| cip_masks(state, x0 as usize, y0 as usize, size, (0, 0)));
    let (mut top, mut left) = build_reference_samples(
        &state.y_plane,
        state.y_stride,
        pic_w,
        pic_h,
        x0 as usize,
        y0 as usize,
        log2_size,
        state.bit_depth,
        avail,
        cip.as_ref().map(|(t, l)| (t.as_slice(), l.as_slice())),
    );

    // Reference sample filtering (spec 8.4.4.2.3). Applied for all non-DC
    // modes at size > 4. The function handles the DC and size exclusions
    // internally. PLANAR (mode 0) IS filtered — this was a prior bug where
    // we only filtered modes 2..34.
    if mode != 1 && !sps.range.intra_smoothing_disabled_flag {
        filter_reference_samples(
            &mut top,
            &mut left,
            log2_size,
            mode,
            sps.strong_intra_smoothing_enabled_flag,
            0, // c_idx = 0 (luma)
            sps.chroma_format_idc,
            state.bit_depth,
        );
    }

    let dst_stride = state.y_stride;
    let dst_offset = (y0 as usize) * dst_stride + (x0 as usize);
    let bit_depth = state.bit_depth;
    with_block(
        &mut state.y_plane,
        &mut state.block_buf,
        state.row_exact,
        dst_offset,
        dst_stride,
        (size, size),
        |dst, stride| {
            match mode {
                0 => predict_planar(dst, stride, &top, &left, log2_size, bit_depth),
                1 => predict_dc(dst, stride, &top, &left, log2_size, edge_filters, bit_depth),
                // The angular edge filters apply to component 0 only; passing a
                // chroma index turns them off.
                2..=34 => predict_angular(
                    dst,
                    stride,
                    &top,
                    &left,
                    log2_size,
                    mode,
                    if edge_filters { 0 } else { 1 },
                    bit_depth,
                ),
                _ => {
                    return Err(DecodeError::Unsupported("invalid intra prediction mode"));
                }
            }
            Ok(())
        },
    )?;
    Ok(())
}

/// Decide which reference-sample directions are available for a luma TU at
/// `(x0, y0)` of size `size`. For Phase 3a-1 we use a simple raster-scan
/// availability rule: a neighbor is available iff its bottom-right pixel
/// has a strictly smaller raster index than `(x0, y0)` AND lies within the
/// picture. Phase 3c-1 adds a cross-slice check: a neighbor pixel belonging
/// to a CTB in a different slice is treated as unavailable, matching the
/// spec rule (`ctb_addr_in_slice > 0` / `>= ctb_width`).
fn compute_luma_avail<P: Pixel>(
    state: &PictureState<P>,
    x0: u32,
    y0: u32,
    size: u32,
) -> ReferenceAvailability {
    compute_luma_avail_inner(state, x0, y0, size, false)
}

/// Core availability computation. When `constrained_intra_pred` is true,
/// additionally requires that all neighbor min-PUs in each direction were
/// intra-coded (pred_flag == 0 in tab_mvf).
fn compute_luma_avail_inner<P: Pixel>(
    state: &PictureState<P>,
    x0: u32,
    y0: u32,
    size: u32,
    constrained_intra_pred: bool,
) -> ReferenceAvailability {
    let pic_w = state.width;
    let pic_h = state.height;
    let log2_ctb = state.log2_ctb_size;
    let ctb_size = 1u32 << log2_ctb;
    let pic_w_in_ctbs = pic_w.div_ceil(ctb_size);

    // Current block's CTB raster address, slice addr, tile id.
    let ctb_x = x0 >> log2_ctb;
    let ctb_y = y0 >> log2_ctb;
    let cur_ctb_rs = ctb_y * pic_w_in_ctbs + ctb_x;
    let cur_slice_addr = state
        .tab_slice_addr_rs
        .get(cur_ctb_rs as usize)
        .copied()
        .unwrap_or(-1);
    let cur_tile_id = state
        .tab_tile_id
        .get(cur_ctb_rs as usize)
        .copied()
        .unwrap_or(0);

    // --- Step 1: CTB-level flags (FFmpeg hls_decode_neighbour) ---
    // Check whether a neighboring CTB is available (decoded, same slice & tile).
    let ctb_avail = |cx: u32, cy: u32| -> bool {
        let rs = cy * pic_w_in_ctbs + cx;
        let sa = state
            .tab_slice_addr_rs
            .get(rs as usize)
            .copied()
            .unwrap_or(-1);
        sa >= 0
            && sa == cur_slice_addr
            && state.tab_tile_id.get(rs as usize).copied().unwrap_or(0) == cur_tile_id
    };
    let ctb_left_flag = ctb_x > 0 && ctb_avail(ctb_x - 1, ctb_y);
    let ctb_up_flag = ctb_y > 0 && ctb_avail(ctb_x, ctb_y - 1);
    let ctb_up_left_flag = ctb_x > 0 && ctb_y > 0 && ctb_avail(ctb_x - 1, ctb_y - 1);
    let ctb_up_right_flag =
        ctb_y > 0 && ctb_x + 1 < pic_w_in_ctbs && ctb_avail(ctb_x + 1, ctb_y - 1);

    // --- Step 2: CU/TU-level flags (FFmpeg ff_hevc_set_neighbour_available) ---
    let x0b = x0 & (ctb_size - 1);
    let y0b = y0 & (ctb_size - 1);

    let cand_up = y0b > 0 || ctb_up_flag;
    let cand_left = x0b > 0 || ctb_left_flag;
    let cand_up_left = if x0b > 0 || y0b > 0 {
        cand_left && cand_up
    } else {
        ctb_up_left_flag
    };

    // cand_up_right: if the TU's right edge reaches exactly the CTB
    // boundary, the up-right samples are in the CTB above-right.
    let cand_up_right = if x0b + size == ctb_size {
        // Cross-CTB: need above-right CTB + must be at CTB top edge.
        ctb_up_right_flag && y0b == 0
    } else {
        cand_up
    };
    // Additional clamp: up-right must be within picture width.
    let cand_up_right = cand_up_right && (x0 + size) < pic_w;

    // cand_bottom_left: if the TU's bottom edge reaches the bottom of the
    // current CTB (or tile/picture bottom), bottom-left is unavailable.
    // FFmpeg uses `end_of_tiles_y = min(y_ctb + ctb_size, height)`.
    let end_of_tiles_y = ((ctb_y + 1) * ctb_size).min(pic_h);
    let cand_bottom_left = if (y0 + size) >= end_of_tiles_y {
        false
    } else {
        cand_left
    };

    // --- Step 3: Z-scan refinement (FFmpeg pred_template.c) ---
    // Within-CTB Z-scan order determines if up-right/bottom-left were
    // decoded before the current block. Use masked coordinates and a
    // Z-interleave function. Sentinel: when a coordinate is -1 (off the
    // CTB edge), return -1 so that `cur > -1` is always true.
    let min_tb_log2 = 2u32; // 4×4 min TB for Main profile
    let log2_diff = (log2_ctb as u32) - min_tb_log2;
    let tb_mask = (1i32 << log2_diff) - 1;

    let x_tb = (x0 >> min_tb_log2) as i32 & tb_mask;
    let y_tb = (y0 >> min_tb_log2) as i32 & tb_mask;
    let size_in_tbs = (size >> min_tb_log2) as i32;

    let zscan = |x: i32, y: i32| -> i32 {
        if x < 0 || y < 0 {
            return -1;
        }
        let mut val = 0i32;
        for i in 0..log2_diff {
            let m = 1i32 << i;
            if x & m != 0 {
                val += m * m;
            }
            if y & m != 0 {
                val += 2 * m * m;
            }
        }
        val
    };

    let cur_z = zscan(x_tb, y_tb);

    let up_right = cand_up_right && cur_z > zscan((x_tb + size_in_tbs) & tb_mask, y_tb - 1);

    let bottom_left = cand_bottom_left && cur_z > zscan(x_tb - 1, (y_tb + size_in_tbs) & tb_mask);

    // --- up, left, up_left use simple raster-scan checks ---
    let up = cand_up && y0 > 0;
    let left = cand_left && x0 > 0;
    let up_left = cand_up_left && x0 > 0 && y0 > 0;

    let avail = ReferenceAvailability {
        up_left,
        up,
        up_right,
        left,
        bottom_left,
    };

    // constrained_intra_pred is applied per sample by the caller through
    // `cip_masks` (spec 8.4.4.2.2 marks individual samples of non-intra
    // blocks unavailable), not per direction here.
    let _ = constrained_intra_pred;
    avail
}

/// Per-sample "from an intra block" flags for constrained intra prediction
/// (spec 8.4.4.2.2): with constrained_intra_pred_flag, a reference sample in
/// a non-intra CU is unavailable. Returns `(top, left)` in the layout of
/// `build_reference_samples` (index 0 is the corner, then the top row
/// rightwards / the left column downwards, 2 * size samples each). `x0`,
/// `y0` are plane sample coordinates; `(sw, sh)` map them to luma.
fn cip_masks<P: Pixel>(
    state: &PictureState<P>,
    x0: usize,
    y0: usize,
    size: usize,
    (sw, sh): (u32, u32),
) -> (Vec<bool>, Vec<bool>) {
    let intra_at = |x: isize, y: isize| -> bool {
        if x < 0 || y < 0 {
            return false;
        }
        let (lx, ly) = ((x as usize) << sw, (y as usize) << sh);
        let idx =
            (ly >> state.log2_min_pu_size) * state.min_pu_width + (lx >> state.log2_min_pu_size);
        state.tab_mvf.get(idx).is_some_and(|m| m.pred_flag == 0)
    };
    let (x0, y0) = (x0 as isize, y0 as isize);
    let corner = intra_at(x0 - 1, y0 - 1);
    let mut top = vec![corner; 2 * size + 1];
    let mut left = vec![corner; 2 * size + 1];
    for i in 0..2 * size {
        top[1 + i] = intra_at(x0 + i as isize, y0 - 1);
        left[1 + i] = intra_at(x0 - 1, y0 + i as isize);
    }
    (top, left)
}

/// Same as `predict_intra_luma` but for one chroma plane (Cb and Cr both
/// use the same logic — different planes, same prediction). The chroma
/// position `(x0, y0)` here is in **luma sample coordinates**; we right-shift
/// by `hshift = vshift = 1` for 4:2:0.
fn predict_intra_chroma<P: Pixel>(
    state: &mut PictureState<P>,
    sps: &Sps,
    x0_luma: u32,
    y0_luma: u32,
    log2_size: u8,
    mode: u8,
    constrained_intra_pred_flag: bool,
) -> Result<(), DecodeError> {
    let size = 1usize << log2_size;
    let pic_w_c = (state.width >> state.chroma_shift_w) as usize;
    let pic_h_c = (state.height >> state.chroma_shift_h) as usize;
    let x_c = (x0_luma >> state.chroma_shift_w) as usize;
    let y_c = (y0_luma >> state.chroma_shift_h) as usize;
    let avail = compute_chroma_avail(
        state,
        x0_luma,
        y0_luma,
        // Availability is computed in luma coordinates; a `size`-wide chroma
        // block spans `size << SubWidthC-shift` luma samples (equal in both
        // dimensions for 4:2:0 and 4:4:4).
        (size as u32) << state.chroma_shift_w,
        constrained_intra_pred_flag,
    );
    let dst_stride = state.uv_stride;
    let cip = constrained_intra_pred_flag.then(|| {
        cip_masks(
            state,
            x_c,
            y_c,
            size,
            (state.chroma_shift_w, state.chroma_shift_h),
        )
    });

    for plane_idx in 0..2 {
        let c_idx = (plane_idx + 1) as u8; // 1 = Cb, 2 = Cr
        let (mut top, mut left) = {
            let src_plane = if plane_idx == 0 {
                &state.u_plane
            } else {
                &state.v_plane
            };
            build_reference_samples(
                src_plane,
                state.uv_stride,
                pic_w_c,
                pic_h_c,
                x_c,
                y_c,
                log2_size,
                state.bit_depth_c,
                avail,
                cip.as_ref().map(|(t, l)| (t.as_slice(), l.as_slice())),
            )
        };

        // Reference sample filtering. For 4:4:4 (ChromaArrayType == 3) chroma
        // is smoothed exactly like luma: PLANAR and angular modes are filtered,
        // DC (mode 1) is not. `filter_reference_samples` no-ops for other chroma
        // formats and for the size-4 case. (Mirrors the luma path; using the
        // narrower `2..=34` here would skip PLANAR, a subtle reconstruction bug.)
        if mode != 1 && !sps.range.intra_smoothing_disabled_flag {
            filter_reference_samples(
                &mut top,
                &mut left,
                log2_size,
                mode,
                sps.strong_intra_smoothing_enabled_flag,
                c_idx,
                sps.chroma_format_idc,
                state.bit_depth_c,
            );
        }

        let (bit_depth_c, row_exact) = (state.bit_depth_c, state.row_exact);
        let plane = if plane_idx == 0 {
            &mut state.u_plane
        } else {
            &mut state.v_plane
        };
        let dst_offset = y_c * dst_stride + x_c;
        with_block(
            plane,
            &mut state.block_buf,
            row_exact,
            dst_offset,
            dst_stride,
            (size, size),
            |dst, stride| {
                match mode {
                    0 => predict_planar(dst, stride, &top, &left, log2_size, bit_depth_c),
                    1 => predict_dc(dst, stride, &top, &left, log2_size, false, bit_depth_c),
                    2..=34 => predict_angular(
                        dst,
                        stride,
                        &top,
                        &left,
                        log2_size,
                        mode,
                        c_idx,
                        bit_depth_c,
                    ),
                    _ => {
                        return Err(DecodeError::Unsupported("invalid intra prediction mode"));
                    }
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}

/// Chroma availability mirrors luma availability — derived from the
/// luma-coordinate position. For 4:2:0 the chroma TU's neighbors are
/// available iff the corresponding luma neighbors were decoded.
fn compute_chroma_avail<P: Pixel>(
    state: &PictureState<P>,
    x0_luma: u32,
    y0_luma: u32,
    luma_size: u32,
    constrained_intra_pred: bool,
) -> ReferenceAvailability {
    compute_luma_avail_inner(state, x0_luma, y0_luma, luma_size, constrained_intra_pred)
}

/// Apply the inverse transform to a luma residual block and add it to the
/// already-predicted luma plane at `(x0, y0)`, with clipping.
///
/// HEVC uses the **4×4 DST** (`transform_4x4_luma`) for intra luma 4×4 TUs;
/// every other size and chroma uses the regular DCT. Since this function is
/// only called from the I-slice intra path, `pred_mode == INTRA` is always
/// true here.
fn apply_residual_to_luma<P: Pixel>(
    state: &mut PictureState<P>,
    x0: u32,
    y0: u32,
    log2_size: u8,
    block: &mut ResidualBlock,
    is_intra: bool,
    keep_for_ccp: bool,
) {
    let size = 1usize << log2_size;
    if let Some(wide) = &block.coeffs_wide {
        // Extended precision with coefficients beyond 16 bits.
        let residual_pixels = crate::inverse_transform::inverse_transform_ext(
            wide,
            log2_size,
            state.bit_depth as u32,
            block.log2_range,
            log2_size == 2 && is_intra,
        );
        let dst_stride = state.y_stride;
        let dst_offset = (y0 as usize) * dst_stride + (x0 as usize);
        let bit_depth = state.bit_depth;
        with_block(
            &mut state.y_plane,
            &mut state.block_buf,
            state.row_exact,
            dst_offset,
            dst_stride,
            (size, size),
            |dst, stride| add_residual(dst, stride, &residual_pixels, log2_size, bit_depth),
        );
        if keep_for_ccp {
            state.ccp_luma_residual = residual_pixels;
        }
        return;
    }
    // The inverse transform runs in place on the block's buffer.
    let residual_pixels = &mut block.coeffs;
    // Spec 8.6.4.2: 4×4 DST is only used for INTRA luma. Inter 4×4 uses
    // regular DCT. Previously this flag was `log2_size == 2`, which silently
    // miscompiled inter 4×4 luma TUs (only reachable with
    // `max_transform_hierarchy_depth_inter >= 2`, i.e. x265 --preset slow).
    if !block.transform_skip && !block.cu_transquant_bypass {
        let is_luma_intra_4x4 = log2_size == 2 && is_intra;
        apply_inverse_transform(
            residual_pixels,
            log2_size,
            block.last_sig_x,
            block.last_sig_y,
            (block.nz_cols, block.nz_rows),
            state.bit_depth as u32,
            is_luma_intra_4x4,
        );
    }
    // When transform_skip, the dequantized coefficients ARE the spatial
    // residual — skip IDCT, add directly (HEVC spec 8.6.4).
    let dst_stride = state.y_stride;
    let dst_offset = (y0 as usize) * dst_stride + (x0 as usize);
    let bit_depth = state.bit_depth;
    with_block(
        &mut state.y_plane,
        &mut state.block_buf,
        state.row_exact,
        dst_offset,
        dst_stride,
        (size, size),
        |dst, stride| add_residual(dst, stride, residual_pixels, log2_size, bit_depth),
    );
    if keep_for_ccp {
        // Copy into the persistent buffer, which keeps its allocation.
        state.ccp_luma_residual.clear();
        state.ccp_luma_residual.extend_from_slice(residual_pixels);
    }
}

/// Pick `scan_idx` for residual_coding (spec 7.4.9.11). For intra TUs at
/// 4×4 / 8×8, certain mode ranges select horizontal or vertical scans. All
/// other cases use diagonal.
fn pick_scan_order(log2_trafo_size: u8, intra_pred_mode: u8) -> ScanOrder {
    if log2_trafo_size > 3 {
        return ScanOrder::Diag;
    }
    // log2_trafo_size in {2, 3}: intra TU. Spec ranges per intra_pred_mode.
    if (6..=14).contains(&intra_pred_mode) {
        ScanOrder::Vert
    } else if (22..=30).contains(&intra_pred_mode) {
        ScanOrder::Horiz
    } else {
        ScanOrder::Diag
    }
}

/// Decode all intra prediction modes for a CU's PUs and write them into
/// `tab_ipm`. Mirrors FFmpeg `intra_prediction_unit` for chroma_format_idc=1.
fn decode_intra_mode_signaling<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    x0: u32,
    y0: u32,
    log2_cb_size: u8,
    part_mode: PartMode,
) -> Result<(), DecodeError> {
    let split = part_mode == PartMode::PartNxN;
    let cb_size = 1u32 << log2_cb_size;
    let pb_size = if split { cb_size >> 1 } else { cb_size };
    let side = if split { 2usize } else { 1 };
    let n_pus = side * side;

    // 1) prev_intra_luma_pred_flag for each PU.
    let mut prev_flag = [false; 4];
    for slot in prev_flag.iter_mut().take(n_pus) {
        *slot = cabac.decode_bin(&mut contexts.state[ctx::PREV_INTRA_LUMA_PRED_FLAG]) != 0;
    }

    // 2) Either mpm_idx (truncated unary, bypass) or rem_intra_luma_pred_mode
    //    (5 bypass bits) for each PU, then derive the actual luma mode.
    let mut intra_pred_mode = [0u8; 4];
    for k in 0..n_pus {
        let (mpm_idx, rem) = if prev_flag[k] {
            // mpm_idx: unary, max value 2 (so up to 2 bypass bits).
            let mut i = 0u8;
            while i < 2 && cabac.decode_bypass() != 0 {
                i += 1;
            }
            (i, 0u8)
        } else {
            (0u8, cabac.decode_bypass_bits(5) as u8)
        };

        let pj = (k % side) as u32;
        let pi = (k / side) as u32;
        let pu_x = x0 + pb_size * pj;
        let pu_y = y0 + pb_size * pi;

        intra_pred_mode[k] =
            compute_luma_intra_pred_mode(state, pu_x, pu_y, prev_flag[k], mpm_idx, rem);

        // Write the mode into tab_ipm so subsequent PUs (and CUs) can read it
        // for their own MPM derivation.
        write_intra_pred_mode(state, pu_x, pu_y, pb_size, intra_pred_mode[k]);
    }

    // 3) intra_chroma_pred_mode. Signalled once for the whole CU, except for
    //    4:4:4 (ChromaArrayType==3) with PART_NxN, where each of the four PUs
    //    carries its own value (spec 7.3.8.5). Each value resolves to a
    //    luma-space mode via the DM rule against its co-located luma PU.
    let n_chroma = if state.chroma_array_type == 3 && split {
        n_pus
    } else {
        1
    };
    let mut chroma_modes = [INTRA_DC; 4];
    for (k, slot) in chroma_modes.iter_mut().enumerate().take(n_chroma) {
        let chroma_mode_idx = decode_intra_chroma_pred_mode(cabac, contexts);
        let luma = intra_pred_mode[k];
        *slot = if chroma_mode_idx == 4 {
            // DM mode: chroma uses the co-located luma PU's mode.
            luma
        } else {
            // Spec table 8-3: chroma_mode_idx → luma-mode space.
            const TABLE: [u8; 4] = [INTRA_PLANAR, INTRA_ANGULAR_26, INTRA_ANGULAR_10, INTRA_DC];
            let mapped = TABLE[chroma_mode_idx as usize];
            if luma == mapped {
                INTRA_ANGULAR_34
            } else {
                mapped
            }
        };
    }

    // Publish the chroma mode(s) per position so the transform unit can look
    // them up. With a single mode it covers the whole CU; the 4:4:4 NxN case
    // writes each PU's mode into its quadrant.
    if n_chroma == 1 {
        write_chroma_pred_mode(state, x0, y0, cb_size, chroma_modes[0]);
    } else {
        for (k, &mode) in chroma_modes.iter().enumerate().take(n_pus) {
            let pj = (k % side) as u32;
            let pi = (k / side) as u32;
            write_chroma_pred_mode(state, x0 + pb_size * pj, y0 + pb_size * pi, pb_size, mode);
        }
    }

    state.last_luma_pred_mode = intra_pred_mode[0];
    state.last_chroma_pred_mode = chroma_modes[0];
    Ok(())
}

fn write_chroma_pred_mode<P: Pixel>(
    state: &mut PictureState<P>,
    x0: u32,
    y0: u32,
    pu_size: u32,
    mode: u8,
) {
    let size_in_pus = (pu_size >> state.log2_min_pu_size).max(1) as usize;
    let x_pu = (x0 >> state.log2_min_pu_size) as usize;
    let y_pu = (y0 >> state.log2_min_pu_size) as usize;
    for j in 0..size_in_pus {
        let row = (y_pu + j) * state.min_pu_width;
        for i in 0..size_in_pus {
            state.tab_chroma_ipm[row + x_pu + i] = mode;
        }
    }
}

fn write_intra_pred_mode<P: Pixel>(
    state: &mut PictureState<P>,
    x0: u32,
    y0: u32,
    pu_size: u32,
    mode: u8,
) {
    let size_in_pus = (pu_size >> state.log2_min_pu_size).max(1) as usize;
    let x_pu = (x0 >> state.log2_min_pu_size) as usize;
    let y_pu = (y0 >> state.log2_min_pu_size) as usize;
    for j in 0..size_in_pus {
        let row = (y_pu + j) * state.min_pu_width;
        for i in 0..size_in_pus {
            state.tab_ipm[row + x_pu + i] = mode;
        }
    }
}

/// `intra_chroma_pred_mode` decode (FFmpeg `ff_hevc_intra_chroma_pred_mode_decode`).
/// Returns 4 for "DM" (chroma uses luma mode), or 0..3 for the chroma table
/// index (spec table 8-3).
fn decode_intra_chroma_pred_mode(cabac: &mut CabacReader, contexts: &mut CabacContexts) -> u8 {
    if cabac.decode_bin(&mut contexts.state[ctx::INTRA_CHROMA_PRED_MODE]) == 0 {
        return 4;
    }
    let hi = cabac.decode_bypass();
    let lo = cabac.decode_bypass();
    ((hi << 1) | lo) as u8
}

/// Luma intra mode derivation with the 3-entry MPM list (HEVC spec 8.4.2).
/// Mirrors FFmpeg `luma_intra_pred_mode`.
fn compute_luma_intra_pred_mode<P: Pixel>(
    state: &PictureState<P>,
    x0: u32,
    y0: u32,
    prev_intra_luma_pred_flag: bool,
    mpm_idx: u8,
    rem_intra_luma_pred_mode: u8,
) -> u8 {
    let log2_ctb = state.log2_ctb_size as u32;
    let x_pu = (x0 >> state.log2_min_pu_size) as usize;
    let y_pu = (y0 >> state.log2_min_pu_size) as usize;
    let y_ctb = (y0 >> log2_ctb) << log2_ctb;

    // An unavailable neighbour (outside the picture, or in another slice or
    // tile) contributes INTRA_DC (spec 8.4.2).
    let mut cand_up = if y_pu > 0 && neighbour_available(state, x0, y0, x0 as i64, y0 as i64 - 1) {
        state.tab_ipm[(y_pu - 1) * state.min_pu_width + x_pu]
    } else {
        INTRA_DC
    };
    let cand_left = if x_pu > 0 && neighbour_available(state, x0, y0, x0 as i64 - 1, y0 as i64) {
        state.tab_ipm[y_pu * state.min_pu_width + x_pu - 1]
    } else {
        INTRA_DC
    };

    // Intra mode prediction does not cross vertical CTB boundaries
    // (FFmpeg comment / spec 8.4.2).
    if (y0 as i64 - 1) < y_ctb as i64 {
        cand_up = INTRA_DC;
    }

    let mut candidate = [0u8; 3];
    if cand_left == cand_up {
        if cand_left < 2 {
            candidate[0] = INTRA_PLANAR;
            candidate[1] = INTRA_DC;
            candidate[2] = INTRA_ANGULAR_26;
        } else {
            // Both neighbors are the same angular mode → derive ±1 angular.
            candidate[0] = cand_left;
            candidate[1] = 2 + (((cand_left as i32) - 2 - 1 + 32) & 31) as u8;
            candidate[2] = 2 + (((cand_left as i32) - 2 + 1) & 31) as u8;
        }
    } else {
        candidate[0] = cand_left;
        candidate[1] = cand_up;
        if candidate[0] != INTRA_PLANAR && candidate[1] != INTRA_PLANAR {
            candidate[2] = INTRA_PLANAR;
        } else if candidate[0] != INTRA_DC && candidate[1] != INTRA_DC {
            candidate[2] = INTRA_DC;
        } else {
            candidate[2] = INTRA_ANGULAR_26;
        }
    }

    if prev_intra_luma_pred_flag {
        candidate[mpm_idx as usize]
    } else {
        // Sort the candidate list ascending, then add `rem_intra_luma_pred_mode`
        // and bump it past every candidate it equals or exceeds.
        if candidate[0] > candidate[1] {
            candidate.swap(0, 1);
        }
        if candidate[0] > candidate[2] {
            candidate.swap(0, 2);
        }
        if candidate[1] > candidate[2] {
            candidate.swap(1, 2);
        }
        let mut mode = rem_intra_luma_pred_mode;
        for c in &candidate {
            if mode >= *c {
                mode += 1;
            }
        }
        mode
    }
}
