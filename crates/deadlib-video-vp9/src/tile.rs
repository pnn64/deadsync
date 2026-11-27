//! Decoding of tiles, superblocks, mode info, and coefficients (tokens)
//! (spec §6.4 "Decode tiles syntax").
//!
//! M3 adds support for inter frames (`FrameIsIntra == 0`); `mode_info()` dispatches to
//! `intra_frame_mode_info()`/`inter_frame_mode_info()` depending on `frame_is_intra`.
//! `inter_frame_mode_info()` (spec §6.4.11-6.4.20) is implemented including motion vector
//! prediction (spec §6.5, `find_mv_refs`/`find_best_ref_mvs`/`append_sub8x8_mvs`), and
//! motion compensation / sub-pixel interpolation is performed via
//! [`predict_inter`](crate::predict::predict_inter) (spec §8.5.2).
//!
//! `TileDecoder::residual` implements `residual()` from spec §6.4.21, and for each plane
//! performs intra prediction ([`crate::predict::predict_intra`]) -> token decoding
//! (`TileDecoder::tokens_and_reconstruct`, spec §6.4.24-6.4.26) -> inverse quantization,
//! inverse transform, and reconstruction (spec §8.6.2), writing the result into the frame
//! buffers in [`TileDecoder::planes`].
//! The loop filter (spec §8.8) is applied separately, via [`TileDecoder::apply_loop_filter`],
//! after all tiles in the frame have been decoded.
//!
//! W5 split this module's implementation across a `tile/` directory of submodules (this file
//! remains the hub: the `TileDecoder` struct/fields and the tile/superblock/partition/block
//! traversal that all submodules are ultimately driven by). The submodules are private (not
//! part of the crate's already-`#[doc(hidden)]` public surface), so referenced here by name
//! only, not as intra-doc links:
//! - `mode_info`: `intra_frame_mode_info`/`inter_frame_mode_info` and everything they call
//!   directly (segment id, skip, tx_size, is_inter, ref_frames, motion vector syntax).
//! - `ref_ctx`: pure neighbor-context derivation for reference-frame syntax elements.
//! - `mv_pred`: motion vector *prediction* (`find_mv_refs` and friends); also absorbs the
//!   former standalone `src/mv.rs`.
//! - `residual`: `residual()` and everything it calls directly (intra/inter prediction
//!   dispatch, token reading, inverse quantization/transform/reconstruction).

use alloc::{boxed::Box, vec, vec::Vec};

mod mode_info;
mod mv_pred;
mod ref_ctx;
mod residual;

pub use mv_pred::Mv;
/// Per-ref-list 4 sub-block MVs for a sub-8x8 inter block (`[ref_list][sub_idx]`).
pub(super) type BlockMvs = [[Mv; 4]; 2];

use alloc::sync::Arc;
use core::fmt;

use crate::bool_coder::{BoolCoderError, BoolDecoder};
use crate::common::INTRA_FRAME;
use crate::compressed_header::{CompressedHeader, CompressedHeaderProbs};
use crate::counts::Counts;
use crate::dpb::RefFrameData;
use crate::framebuffer::Plane;
use crate::header::{self, ColorConfig, NewFrameHeader, SegmentationParams, MAX_SEGMENTS};
use crate::prob_tables::{
    BLOCK_64X64, BLOCK_8X8, BLOCK_INVALID, B_HEIGHT_LOG2_LOOKUP, B_WIDTH_LOG2_LOOKUP, DC_PRED,
    KF_PARTITION_PROBS, LAST_FRAME, MI_WIDTH_LOG2_LOOKUP, NUM_8X8_BLOCKS_HIGH_LOOKUP,
    NUM_8X8_BLOCKS_WIDE_LOOKUP, PARTITION_HORZ, PARTITION_NONE, PARTITION_SPLIT, PARTITION_TREE,
    PARTITION_VERT, REF_NONE, SS_SIZE_LOOKUP, SUBSIZE_LOOKUP, TX_4X4,
};

/// Errors that can occur while decoding tiles/partitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileError {
    /// Failed to initialize the bool decoder for the tile data.
    BoolCoder(BoolCoderError),
    /// Tile partitioning is invalid, e.g. the tile size field exceeds the data length.
    InvalidTileSize,
    /// A size lookup (`subsize_lookup`/`ss_size_lookup`) returned `BLOCK_INVALID` -- the block
    /// size is inconsistent for this frame's chroma subsampling (malformed bitstream).
    InvalidPartition,
    /// An inter block references a DPB slot that holds no decoded frame (malformed bitstream).
    MissingReference,
    /// An inter block references a frame outside VP9 spec §8.5.2.3's scaling bounds
    /// (`2 * FrameWidth >= RefFrameWidth`, `16 * RefFrameWidth >= FrameWidth`, and the height
    /// analogs; or zero dimensions). The 2x upper bound caps the motion-compensation scaling
    /// step at 32 (1/16-pel units), sizing every scaled inter-prediction scratch buffer
    /// (scalar and AVX2; see `subpel::MAX_INTERMEDIATE_HEIGHT`). Checked per block, not at
    /// reference resolution: a conformant stream may *list* an out-of-range slot it never
    /// predicts from (3-layer SVC does; see `Decoder::decode_one_frame`).
    RefFrameSizeOutOfRange,
    /// The tile's arithmetic decoder ran far past the end of the tile buffer (or left a large
    /// non-zero unused tail). A conformant tile holds just enough coded bits for its blocks;
    /// running thousands of bits past the end means the decode desynced on corrupt data (libvpx
    /// rejects the same via its reader's `has_error`).
    CorruptTile,
}

impl From<BoolCoderError> for TileError {
    fn from(e: BoolCoderError) -> Self {
        TileError::BoolCoder(e)
    }
}

impl fmt::Display for TileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TileError::BoolCoder(e) => write!(f, "tile bool decoder error: {e}"),
            TileError::InvalidTileSize => f.write_str("invalid or truncated tile size"),
            TileError::InvalidPartition => f.write_str("invalid block partition for subsampling"),
            TileError::MissingReference => {
                f.write_str("inter block references an uninitialized DPB slot")
            }
            TileError::RefFrameSizeOutOfRange => {
                f.write_str("reference frame exceeds 2x scaling bound")
            }
            TileError::CorruptTile => {
                f.write_str("corrupt tile data (arithmetic decoder out of sync)")
            }
        }
    }
}

impl core::error::Error for TileError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            TileError::BoolCoder(e) => Some(e),
            _ => None,
        }
    }
}

/// Over-read (bits requested past the tile buffer end) beyond which a tile is treated as
/// corrupt. Conformant streams measure 0 here (the whole official corpus); the bound sits well
/// above the handful of padding bits a valid final renorm could pull, and far below the
/// thousands a desynced decode runs up. See [`TileError::CorruptTile`].
const TILE_OVER_READ_LIMIT_BITS: usize = 128;

/// Unused tail (buffer bits the tile decode left unconsumed) beyond which a tile is treated as
/// corrupt. Conformant tiles leave at most 14 bits (measured across the whole official corpus);
/// a desynced decode of corrupt data finishes the frame's fixed block count thousands of bits
/// short (the smallest such case observed left ~10k). See [`TileError::CorruptTile`].
const TILE_UNDER_READ_LIMIT_BITS: usize = 1024;

/// Returns `true` if every unread trailing bit in `tile_bytes` starting at bit index `bit_pos`
/// (MSB-first within each byte) is zero, matching spec §9.2.3 `exit_bool()`'s zero-padding rule.
fn trailing_bits_are_zero(tile_bytes: &[u8], bit_pos: usize) -> bool {
    let byte_idx = bit_pos / 8;
    if byte_idx >= tile_bytes.len() {
        return true;
    }
    let bit_rem = bit_pos % 8;
    let tail = if bit_rem == 0 {
        &tile_bytes[byte_idx..]
    } else {
        let mask = 0xFFu8 >> bit_rem;
        if (tile_bytes[byte_idx] & mask) != 0 {
            return false;
        }
        &tile_bytes[byte_idx + 1..]
    };
    tail.iter().all(|&b| b == 0)
}

/// Validates tile-size headers, non-empty tile payloads, and bool-decoder marker bits before
/// allocating per-frame working buffers (`Plane`s / `MiGrid`).
pub(crate) fn validate_tile_layout(
    mut data: &[u8],
    tile_cols_log2: u32,
    tile_rows_log2: u32,
) -> Result<(), TileError> {
    let tile_cols = 1usize
        .checked_shl(tile_cols_log2)
        .ok_or(TileError::InvalidTileSize)?;
    let tile_rows = 1usize
        .checked_shl(tile_rows_log2)
        .ok_or(TileError::InvalidTileSize)?;
    let num_tiles = tile_cols
        .checked_mul(tile_rows)
        .ok_or(TileError::InvalidTileSize)?;
    let min_bytes = num_tiles
        .checked_mul(5)
        .and_then(|v| v.checked_sub(4))
        .ok_or(TileError::InvalidTileSize)?;
    if data.len() < min_bytes {
        if num_tiles == 1 && data.is_empty() {
            return Err(TileError::BoolCoder(BoolCoderError::EmptyBuffer));
        }
        return Err(TileError::InvalidTileSize);
    }
    for i in 0..num_tiles {
        let tile_bytes = take_tile(&mut data, i + 1 == num_tiles)?;
        let Some(&first) = tile_bytes.first() else {
            return Err(TileError::BoolCoder(BoolCoderError::EmptyBuffer));
        };
        // `BoolDecoder::new` requires the first `read_bool(128)` marker bit (MSB of byte 0) == 0.
        if (first & 0x80) != 0 {
            return Err(TileError::BoolCoder(BoolCoderError::InvalidMarker));
        }
    }
    Ok(())
}

fn take_tile<'a>(data: &mut &'a [u8], last: bool) -> Result<&'a [u8], TileError> {
    let size = if last {
        data.len()
    } else {
        let (size, rest) = data
            .split_first_chunk::<4>()
            .ok_or(TileError::InvalidTileSize)?;
        *data = rest;
        u32::from_be_bytes(*size) as usize
    };
    let (tile, rest) = data
        .split_at_checked(size)
        .ok_or(TileError::InvalidTileSize)?;
    *data = rest;
    Ok(tile)
}

/// Rejects a decoded tile whose arithmetic decoder finished far off its buffer end.
///
/// A conformant tile's arithmetic decoder finishes flush against its buffer: the
/// whole official corpus lands within 14 bits of the end and never over-reads.
/// A tile whose coded data has been corrupted desyncs and ends far off that mark
/// -- either running past the end (`over_read_bits`), or finishing the frame's
/// fixed block count leaving a large non-zero unused tail. Spec-legal trailing
/// zero padding (spec §9.2.3 `exit_bool()`) of arbitrary length is accepted when
/// every remaining unread tail bit is zero; any non-zero bit in a large unused
/// tail is rejected as [`TileError::CorruptTile`].
fn check_tile_read_bounds(r: &BoolDecoder, tile_bytes: &[u8]) -> Result<(), TileError> {
    if r.over_read_bits() > TILE_OVER_READ_LIMIT_BITS {
        return Err(TileError::CorruptTile);
    }
    let bit_pos = r.bit_position();
    let unused_bits = (tile_bytes.len() * 8).saturating_sub(bit_pos);
    if unused_bits > TILE_UNDER_READ_LIMIT_BITS && !trailing_bits_are_zero(tile_bytes, bit_pos) {
        return Err(TileError::CorruptTile);
    }
    Ok(())
}

/// Sentinel in [`MiInfo::extra`] indicating an inter block has no distinct sub-8x8 MVs
/// (all 4 sub-blocks share [`MiInfo::mv`]).
pub(super) const NO_SUB_MVS: u32 = u32::MAX;

/// Information held by a single 8x8 mode info unit (spec §2.37 "Mode info"), packed into 24 bytes.
///
/// Bit packing preserves exact value ranges:
/// - `size_mode`: `mi_size` (0..=12, bits 0..4) | `y_mode` (0..=13, bits 4..8)
/// - `ref_tx_interp`: `ref_frame[0]` (0..=3, bits 0..2) | `ref_frame[1]` (0..=3, bits 2..4)
///   | `tx_size` (0..=3, bits 4..6) | `interp_filter` (0..=3, bits 6..8)
/// - `seg_skip_uv`: `segment_id` (0..=7, bits 0..3) | `skip` (bit 3) | `uv_mode` (0..=9, bits 4..8)
/// - `extra`: for intra blocks (`ref_frame0() == INTRA_FRAME`), 4 packed `sub_modes` bytes;
///   for inter blocks, an index into the owning `MiGrid::sub_mvs` when `mi_size() < BLOCK_8X8`
///   else `NO_SUB_MVS`. The table is cleared before the grid is retained for the next frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MiInfo {
    /// `Mvs[ refList ]` (representative MV == `BlockMvs[ refList ][ 3 ]`, spec §6.4.4).
    /// Units are 1/8 pel. `[[0, 0]; 2]` for intra blocks.
    pub mv: [[i32; 2]; 2],
    extra: u32,
    size_mode: u8,
    ref_tx_interp: u8,
    seg_skip_uv: u8,
    _pad: u8,
}

impl MiInfo {
    #[inline]
    pub(super) fn new_intra(
        skip: bool,
        tx_size: u8,
        mi_size: u8,
        y_mode: u8,
        uv_mode: u8,
        sub_modes: [u8; 4],
        segment_id: u8,
    ) -> Self {
        debug_assert!(mi_size < 16 && y_mode < 16 && tx_size < 4 && segment_id < 8 && uv_mode < 16);
        Self {
            mv: [[0, 0]; 2],
            extra: u32::from_ne_bytes(sub_modes),
            size_mode: mi_size | (y_mode << 4),
            ref_tx_interp: INTRA_FRAME | (REF_NONE << 2) | (tx_size << 4),
            seg_skip_uv: segment_id | ((skip as u8) << 3) | (uv_mode << 4),
            _pad: 0,
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_inter(
        skip: bool,
        tx_size: u8,
        mi_size: u8,
        y_mode: u8,
        segment_id: u8,
        ref_frame: [u8; 2],
        mv: [[i32; 2]; 2],
        interp_filter: u8,
    ) -> Self {
        debug_assert!(
            mi_size < 16
                && y_mode < 16
                && ref_frame[0] < 4
                && ref_frame[1] < 4
                && tx_size < 4
                && interp_filter < 4
                && segment_id < 8
        );
        Self {
            mv,
            extra: NO_SUB_MVS,
            size_mode: mi_size | (y_mode << 4),
            ref_tx_interp: ref_frame[0]
                | (ref_frame[1] << 2)
                | (tx_size << 4)
                | (interp_filter << 6),
            seg_skip_uv: segment_id | ((skip as u8) << 3),
            _pad: 0,
        }
    }

    #[inline]
    pub fn skip(&self) -> bool {
        (self.seg_skip_uv & 0x08) != 0
    }

    #[inline]
    pub(super) fn set_skip(&mut self, skip: bool) {
        if skip {
            self.seg_skip_uv |= 0x08;
        } else {
            self.seg_skip_uv &= !0x08;
        }
    }

    #[inline]
    pub fn tx_size(&self) -> u8 {
        (self.ref_tx_interp >> 4) & 0x03
    }

    #[inline]
    pub fn mi_size(&self) -> u8 {
        self.size_mode & 0x0F
    }

    #[inline]
    pub fn y_mode(&self) -> u8 {
        self.size_mode >> 4
    }

    #[inline]
    pub fn uv_mode(&self) -> u8 {
        self.seg_skip_uv >> 4
    }

    #[inline]
    pub fn segment_id(&self) -> u8 {
        self.seg_skip_uv & 0x07
    }

    #[inline]
    pub fn ref_frame0(&self) -> u8 {
        self.ref_tx_interp & 0x03
    }

    #[inline]
    pub fn ref_frame(&self) -> [u8; 2] {
        [self.ref_tx_interp & 0x03, (self.ref_tx_interp >> 2) & 0x03]
    }

    #[inline]
    pub fn interp_filter(&self) -> u8 {
        self.ref_tx_interp >> 6
    }

    #[inline]
    pub fn sub_modes(&self) -> [u8; 4] {
        if self.ref_frame0() == INTRA_FRAME {
            self.extra.to_ne_bytes()
        } else {
            [DC_PRED; 4]
        }
    }
}

impl Default for MiInfo {
    fn default() -> Self {
        Self::new_intra(
            false,
            TX_4X4,
            crate::prob_tables::BLOCK_4X4,
            DC_PRED,
            DC_PRED,
            [DC_PRED; 4],
            0,
        )
    }
}

/// A grid holding mode info for the whole frame, in 8x8 units.
///
/// This bundles the spec's `Skips`/`TxSizes`/`MiSizes`/`YModes`/`SegmentIds`/`SubModes`
/// arrays into a single compact struct. Its size is `Sb64Cols*8 x Sb64Rows*8`
/// (allocated to also cover the portion of edge superblocks that extends past the frame).
///
/// Not `Clone`: the previous frame's grid is shared into the next frame's `TileDecoder` via
/// `Arc<MiGrid>` (see `crate::Decoder::prev_mi_grid`) rather than deep-cloned.
pub struct MiGrid {
    cols: usize,
    rows: usize,
    /// Row stride in mode-info units.
    stride: usize,
    /// Absolute MI column of this grid's first column (0 for a whole-frame grid).
    col0: u32,
    cells: Vec<MiInfo>,
    /// Sparse storage of sub-block MVs `0..3` (sub-block `3` is [`MiInfo::mv`]) for inter blocks
    /// with `mi_size < BLOCK_8X8`. Only populated during tile decode; cleared before storing as
    /// `prev_mi_grid` (which only reads `mv` and `ref_frame`).
    sub_mvs: Vec<[[Mv; 3]; 2]>,
}

impl fmt::Debug for MiGrid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MiGrid")
            .field("cols", &self.cols)
            .field("rows", &self.rows)
            .field("stride", &self.stride)
            .field("col0", &self.col0)
            .finish_non_exhaustive()
    }
}

impl MiGrid {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            stride: cols,
            col0: 0,
            cells: vec![MiInfo::default(); cols * rows],
            sub_mvs: Vec::new(),
        }
    }

    /// Reuses a pooled whole-frame `MiGrid` for `(cols, rows)` if its capacity suffices (without
    /// retaining grossly oversized buffers after a large down-resize).
    fn try_reuse(&mut self, cols: usize, rows: usize) -> bool {
        let data = &mut self.cells;
        let need = cols * rows;
        if data.capacity() >= need && data.capacity() <= need.saturating_mul(4).max(1024) {
            self.cols = cols;
            self.rows = rows;
            self.stride = cols;
            self.col0 = 0;
            data.resize(need, MiInfo::default());
            self.sub_mvs.clear();
            true
        } else {
            false
        }
    }

    /// Clears the transient sub-8x8 MV side table before archiving this grid as `prev_mi_grid`
    /// (temporal MV prediction only reads representative `MiInfo::mv` and `ref_frame`).
    pub(crate) fn clear_sub_mvs(&mut self) {
        self.sub_mvs.clear();
    }

    #[inline]
    pub(super) fn push_sub_mvs(&mut self, block_mvs: &[[Mv; 4]; 2]) -> u32 {
        let idx = self.sub_mvs.len() as u32;
        self.sub_mvs.push([
            [block_mvs[0][0], block_mvs[0][1], block_mvs[0][2]],
            [block_mvs[1][0], block_mvs[1][1], block_mvs[1][2]],
        ]);
        idx
    }

    /// Returns sub-block `idx`'s MV for `ref_list` (`idx == 3` or blocks without distinct
    /// sub-8x8 MVs return `info.mv[ref_list]`).
    ///
    /// The current frame's grid owns these vectors until tile decoding finishes.
    #[inline]
    pub(super) fn get_sub_mv(&self, info: &MiInfo, ref_list: usize, idx: usize) -> Mv {
        if idx == 3 || info.extra == NO_SUB_MVS {
            info.mv[ref_list]
        } else {
            debug_assert!(
                (info.extra as usize) < self.sub_mvs.len(),
                "MiInfo::extra ({}) out of range for sub_mvs (len={})",
                info.extra,
                self.sub_mvs.len()
            );
            self.sub_mvs[info.extra as usize][ref_list][idx]
        }
    }

    #[inline]
    fn base_ptr(&self) -> *const MiInfo {
        self.cells.as_ptr()
    }

    #[inline]
    fn base_ptr_mut(&mut self) -> *mut MiInfo {
        self.cells.as_mut_ptr()
    }

    /// Unchecked MI lookup for hot decode/loop-filter paths.
    ///
    /// # Safety
    /// Caller must ensure `(row as usize) < self.rows`, `col >= self.col0`, and
    /// `((col - self.col0) as usize) < self.cols`.
    #[inline]
    pub(crate) unsafe fn get_unchecked(&self, row: u32, col: u32) -> &MiInfo {
        debug_assert!(
            (row as usize) < self.rows
                && col >= self.col0
                && ((col - self.col0) as usize) < self.cols,
            "MiGrid::get_unchecked out of range"
        );
        // SAFETY: caller guarantees `(row, col - self.col0)` lies inside `[0..self.rows) x [0..self.cols)`.
        &*self
            .base_ptr()
            .add(row as usize * self.stride + (col - self.col0) as usize)
    }

    /// Fills rectangle `[row..row + bh) x [col..col + bw)` with `info` without release bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `col >= self.col0`, `((col - self.col0) as usize) + bw as usize <= self.cols`,
    /// and `(row as usize) + (bh as usize) <= self.rows`.
    #[inline]
    unsafe fn fill_rect_unchecked(&mut self, row: u32, col: u32, bw: u32, bh: u32, info: MiInfo) {
        debug_assert!(
            col >= self.col0
                && ((col - self.col0) as usize) + (bw as usize) <= self.cols
                && (row as usize) + (bh as usize) <= self.rows,
            "MiGrid::fill_rect_unchecked out of range"
        );
        let c = (col - self.col0) as usize;
        let bw = bw as usize;
        let stride = self.stride;
        let base = self.base_ptr_mut();
        for r in 0..bh as usize {
            // SAFETY: caller guarantees each row slice `[c..c + bw]` lies strictly within this
            // grid/strip's disjoint column range `[0..self.cols)` and `row + r < self.rows`.
            core::slice::from_raw_parts_mut(base.add((row as usize + r) * stride + c), bw)
                .fill(info);
        }
    }
}

/// Reusable per-block token/dequant scratch (heap-boxed once and pooled across frames to keep
/// `TileDecoder` tiny on the stack and avoid 9 KB moves).
pub(crate) struct TileScratch {
    pub(super) dequant_buf: [i64; 1024],
    pub(super) token_cache_buf: [u8; 1024],
}

impl TileScratch {
    pub(crate) fn new() -> Box<Self> {
        Box::new(Self {
            dequant_buf: [0i64; 1024],
            token_cache_buf: [0u8; 1024],
        })
    }
}

/// Reusable frame-width above-context rows (`AbovePartitionContext`, `AboveSegPredContext`,
/// `AboveNonzeroContext`), pooled across frames in `Decoder` to avoid 5 heap allocations/frame.
pub(crate) struct AboveContexts {
    pub(super) partition: Vec<u8>,
    pub(super) seg_pred: Vec<u8>,
    pub(super) nonzero: [Vec<u8>; 3],
}

impl AboveContexts {
    pub(crate) fn new(grid_cols: usize) -> Self {
        let mut ctx = Self {
            partition: Vec::new(),
            seg_pred: Vec::new(),
            nonzero: [Vec::new(), Vec::new(), Vec::new()],
        };
        ctx.reset_for(grid_cols);
        ctx
    }

    pub(crate) fn reset_for(&mut self, grid_cols: usize) {
        for (buf, len) in [
            (&mut self.partition, grid_cols),
            (&mut self.seg_pred, grid_cols),
        ] {
            buf.resize(len, 0);
            buf.fill(0);
        }
        let nz_len = grid_cols * 2;
        for ctx in &mut self.nonzero {
            ctx.resize(nz_len, 0);
            ctx.fill(0);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.partition.fill(0);
        self.seg_pred.fill(0);
        for ctx in &mut self.nonzero {
            ctx.fill(0);
        }
    }
}

/// Optional caller-pooled buffers passed into [`TileDecoder::new_with_buffers`] to avoid per-frame
/// heap allocations across consecutive frames.
#[derive(Default)]
pub(crate) struct RecycledBuffers {
    pub(crate) planes: Option<[Plane; 3]>,
    pub(crate) mi_grid: Option<MiGrid>,
    pub(crate) above_ctx: Option<AboveContexts>,
    pub(crate) scratch: Option<Box<TileScratch>>,
    pub(crate) counts: Option<Box<Counts>>,
}

/// `get_tile_offset( tileNum, mis, tileSzLog2 )` (spec §6.4.1).
fn get_tile_offset(tile_num: u32, mis: u32, tile_sz_log2: u32) -> u32 {
    let sbs = (mis + 7) >> 3;
    let offset = ((tile_num * sbs) >> tile_sz_log2) << 3;
    offset.min(mis)
}

/// Decoder that walks tiles and superblocks to decode mode info.
///
/// Holds per-frame state (mode info grid, above/left partition context).
pub struct TileDecoder<'p> {
    tx_mode: u8,
    /// Immutable probability tables for this frame.
    probs: &'p CompressedHeaderProbs,
    segmentation: SegmentationParams,
    mi_cols: u32,
    mi_rows: u32,
    tile_cols_log2: u32,
    tile_rows_log2: u32,
    mi_grid: MiGrid,
    /// Frame-width above contexts (`AbovePartitionContext`, `AboveSegPredContext`,
    /// `AboveNonzeroContext`), pooled across frames.
    above_ctx: AboveContexts,
    /// `LeftPartitionContext` (per-SB row, indexed mod 8).
    left_partition_context: [u8; 8],
    /// `LeftSegPredContext` (per-SB row, indexed mod 8).
    left_seg_pred_context: [u8; 8],
    /// `PrevSegmentIds[ MiRow ][ MiCol ]` (spec §6.4.14 `get_segment_id`), in
    /// row-major `MiRows x MiCols` layout (unpadded, unlike `mi_grid`). Supplied
    /// by the caller ([`Decoder`](crate::Decoder)), which is responsible for the
    /// size-change-clears-to-zero rule of spec §7.2.6. `Arc`-shared with the caller's
    /// copy (never mutated here) to avoid a clone-in every frame.
    prev_segment_ids: Arc<Vec<u8>>,
    // Range of the tile currently being decoded.
    mi_col_start: u32,
    mi_col_end: u32,
    mi_row_start: u32,
    mi_row_end: u32,

    // --- Additional state needed for coefficient decoding/reconstruction (spec §6.4.21-6.4.26, §8.5-8.6). ---
    bit_depth: u8,
    subsampling_x: u32,
    subsampling_y: u32,
    lossless: bool,
    /// Per-frame dequant step table (see [`residual::build_dequant_table`]); replaces
    /// re-deriving `get_qindex`/`get_dc_quant`/`get_ac_quant` on every transform block.
    dequant_table: [[[i64; 2]; 2]; MAX_SEGMENTS],
    /// `CurrFrame[ plane ]`. Index 0=Y, 1=U, 2=V.
    planes: [Plane; 3],
    /// `LeftNonzeroContext[ plane ]`. Cleared per superblock row and only ever accessed at
    /// the relative position within that superblock row (absolute 4x4 row number mod 16),
    /// so it is kept as a 16-element array (64 pixels / 4 = 16).
    left_nonzero_context: [[u8; 16]; 3],

    // --- Additional state needed for inter frames (spec §6.4.11-6.4.20, §6.5). ---
    /// `FrameIsIntra`.
    frame_is_intra: bool,
    /// `ref_frame_sign_bias[ 0..4 ]` (from the uncompressed header).
    ref_frame_sign_bias: [bool; 4],
    allow_high_precision_mv: bool,
    /// `interpolation_filter` (when `SWITCHABLE`, `interp_filter` is read per block).
    interpolation_filter: u8,
    /// `reference_mode`/`CompFixedRef`/`CompVarRef` (from the compressed header, spec §6.3.12/6.3.18).
    reference_mode: u8,
    comp_fixed_ref: u8,
    comp_var_ref: [u8; 2],
    /// `UsePrevFrameMvs` (spec §7.2.6). When true, `Mvs`/`RefFrames` of the previous frame
    /// are referenced via `prev_mi_grid`.
    use_prev_frame_mvs: bool,
    /// `MiGrid` of the most recently decoded frame (equivalent to `PrevMvs`/`PrevRefFrames`).
    /// Not referenced when `use_prev_frame_mvs == false` (may be `None`). `Arc`-shared with
    /// the caller's copy (read-only here) to avoid a clone-in every frame.
    prev_mi_grid: Option<Arc<MiGrid>>,

    // --- Additional state needed for motion compensation (spec §8.5.2). ---
    /// `FrameWidth`/`FrameHeight` (actual size used for display/scaling calculations, distinct
    /// from the padded size such as `mi_cols*8`).
    frame_width: u32,
    frame_height: u32,
    /// Actual pixel data of the reference frames used to decode this frame (already resolved
    /// from the DPB by the caller using `header.ref_frame_idx`). Indexed by the `ref_frame`
    /// value `LAST_FRAME..=ALTREF_FRAME` minus `LAST_FRAME`. All elements are `None` when
    /// `FrameIsIntra == 1`. `Arc`-shared with the DPB slot(s) they were resolved from,
    /// rather than deep-cloned per reference.
    resolved_refs: [Option<Arc<RefFrameData>>; 3],

    // --- Lazy syntax-element counters (spec §8.4, §9.3.4): `None` when backward adaptation is off. ---
    counts: Option<Box<Counts>>,
    // --- Reusable heap-boxed per-block scratch for `tokens_and_reconstruct`. ---
    scratch: Box<TileScratch>,
}

impl<'p> TileDecoder<'p> {
    /// Creates frame-local decode state with reusable scratch buffers.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_buffers(
        header: &NewFrameHeader,
        color_config: ColorConfig,
        compressed: &'p CompressedHeader,
        use_prev_frame_mvs: bool,
        prev_mi_grid: Option<Arc<MiGrid>>,
        resolved_refs: [Option<Arc<RefFrameData>>; 3],
        prev_segment_ids: Arc<Vec<u8>>,
        collect_counts: bool,
        recycled: RecycledBuffers,
    ) -> Self {
        let image_size = header::compute_image_size(header.width, header.height);
        let grid_cols = (image_size.sb64_cols * 8) as usize;
        let grid_rows = (image_size.sb64_rows * 8) as usize;
        let subsampling_x = color_config.subsampling_x as u32;
        let subsampling_y = color_config.subsampling_y as u32;

        let y_w = (image_size.sb64_cols * 64) as usize;
        let y_h = (image_size.sb64_rows * 64) as usize;
        let uv_w = y_w >> subsampling_x;
        let uv_h = y_h >> subsampling_y;
        let planes = recycled
            .planes
            .and_then(|mut p| {
                let cap_ok = p[0].try_reuse_as(y_w, y_h)
                    && p[1].try_reuse_as(uv_w, uv_h)
                    && p[2].try_reuse_as(uv_w, uv_h);
                cap_ok.then_some(p)
            })
            .unwrap_or_else(|| {
                [
                    Plane::new(y_w, y_h),
                    Plane::new(uv_w, uv_h),
                    Plane::new(uv_w, uv_h),
                ]
            });
        let mi_grid = recycled
            .mi_grid
            .and_then(|mut g| g.try_reuse(grid_cols, grid_rows).then_some(g))
            .unwrap_or_else(|| MiGrid::new(grid_cols, grid_rows));
        let above_ctx = match recycled.above_ctx {
            Some(mut ctx) => {
                ctx.reset_for(grid_cols);
                ctx
            }
            None => AboveContexts::new(grid_cols),
        };
        let scratch = recycled.scratch.unwrap_or_else(TileScratch::new);
        let counts = if collect_counts {
            Some(match recycled.counts {
                Some(mut c) => {
                    c.reset();
                    c
                }
                None => Box::new(Counts::new()),
            })
        } else {
            None
        };

        Self {
            tx_mode: compressed.tx_mode,
            probs: &compressed.probs,
            segmentation: header.segmentation,
            mi_cols: image_size.mi_cols,
            mi_rows: image_size.mi_rows,
            tile_cols_log2: header.tile_cols_log2,
            tile_rows_log2: header.tile_rows_log2,
            mi_grid,
            above_ctx,
            left_partition_context: [0u8; 8],
            left_seg_pred_context: [0u8; 8],
            prev_segment_ids,
            mi_col_start: 0,
            mi_col_end: image_size.mi_cols,
            mi_row_start: 0,
            mi_row_end: image_size.mi_rows,
            bit_depth: color_config.bit_depth,
            subsampling_x,
            subsampling_y,
            lossless: header.quantization.lossless,
            dequant_table: residual::build_dequant_table(
                &header.segmentation,
                header.quantization.base_q_idx,
                color_config.bit_depth,
                header.quantization.delta_q_y_dc,
                header.quantization.delta_q_uv_dc,
                header.quantization.delta_q_uv_ac,
            ),
            planes,
            left_nonzero_context: [[0u8; 16]; 3],
            frame_is_intra: header.frame_is_intra,
            ref_frame_sign_bias: header.ref_frame_sign_bias,
            allow_high_precision_mv: header.allow_high_precision_mv,
            interpolation_filter: header.interpolation_filter,
            reference_mode: compressed.reference_mode,
            comp_fixed_ref: compressed.comp_fixed_ref,
            comp_var_ref: compressed.comp_var_ref,
            use_prev_frame_mvs,
            prev_mi_grid,
            frame_width: header.width,
            frame_height: header.height,
            resolved_refs,
            counts,
            scratch,
        }
    }

    /// Consumes `self`, dropping its read-only reference-frame and previous-grid `Arc` handles
    /// and handing back its reusable buffers for zero-copy DPB storage and cross-frame pooling.
    pub(crate) fn into_parts(
        self,
    ) -> (
        [Plane; 3],
        MiGrid,
        AboveContexts,
        Box<TileScratch>,
        Option<Box<Counts>>,
    ) {
        (
            self.planes,
            self.mi_grid,
            self.above_ctx,
            self.scratch,
            self.counts,
        )
    }

    /// Invokes `f` on `self.counts` when backward-adaptation counting is enabled.
    #[inline(always)]
    pub(super) fn count(&mut self, f: impl FnOnce(&mut Counts)) {
        if let Some(counts) = self.counts.as_deref_mut() {
            f(counts);
        }
    }

    /// The collected syntax element counters (used for probability adaptation, spec §8.4).
    /// Panics if count collection was disabled for this frame.
    pub fn counts(&self) -> &Counts {
        self.counts
            .as_deref()
            .expect("counts() called with count collection disabled")
    }

    pub fn mi_grid(&self) -> &MiGrid {
        &self.mi_grid
    }

    /// Applies the loop filter (spec §8.8, [`crate::loop_filter`]) to the decoded `planes`.
    pub fn apply_loop_filter(&mut self, lf: &header::LoopFilterParams) {
        crate::loop_filter::loop_filter_frame(
            &mut self.planes,
            &self.mi_grid,
            self.mi_cols,
            self.mi_rows,
            self.subsampling_x,
            self.subsampling_y,
            lf,
            &self.segmentation,
            self.bit_depth,
        );
    }

    /// `clear_above_context( )` (spec §7.4.1): also clears `AboveSegPredContext`.
    fn clear_above_context(&mut self) {
        self.above_ctx.clear();
    }

    /// `clear_left_context( )` (spec §7.4.2): also clears `LeftSegPredContext`.
    fn clear_left_context(&mut self) {
        self.left_partition_context = [0u8; 8];
        self.left_seg_pred_context = [0u8; 8];
        self.left_nonzero_context = [[0u8; 16]; 3];
    }

    /// `decode_tiles( sz )` (spec §6.4). `data` is the entire tile data following the
    /// uncompressed header, excluding the compressed header (the rest of the frame data).
    pub fn decode_tiles(&mut self, mut data: &[u8]) -> Result<(), TileError> {
        let tile_cols = 1u32 << self.tile_cols_log2;
        let tile_rows = 1u32 << self.tile_rows_log2;

        self.clear_above_context();

        for tile_row in 0..tile_rows {
            for tile_col in 0..tile_cols {
                let last = tile_row + 1 == tile_rows && tile_col + 1 == tile_cols;
                let tile_bytes = take_tile(&mut data, last)?;

                self.mi_row_start = get_tile_offset(tile_row, self.mi_rows, self.tile_rows_log2);
                self.mi_row_end = get_tile_offset(tile_row + 1, self.mi_rows, self.tile_rows_log2);
                self.mi_col_start = get_tile_offset(tile_col, self.mi_cols, self.tile_cols_log2);
                self.mi_col_end = get_tile_offset(tile_col + 1, self.mi_cols, self.tile_cols_log2);

                self.decode_tile_bytes(tile_bytes)?;
            }
        }
        Ok(())
    }

    /// Decodes one tile and checks the arithmetic reader's final position.
    fn decode_tile_bytes(&mut self, tile_bytes: &[u8]) -> Result<(), TileError> {
        let mut r = BoolDecoder::new(tile_bytes)?;
        self.decode_tile(&mut r)?;
        check_tile_read_bounds(&r, tile_bytes)?;
        r.exit_bool();
        Ok(())
    }

    /// `decode_tile( )` (spec §6.4.2).
    fn decode_tile(&mut self, r: &mut BoolDecoder) -> Result<(), TileError> {
        let mut row = self.mi_row_start;
        while row < self.mi_row_end {
            self.clear_left_context();
            let mut col = self.mi_col_start;
            while col < self.mi_col_end {
                self.decode_partition(r, row, col, BLOCK_64X64)?;
                col += 8;
            }
            // Fail fast on severely truncated tiles (monotonically non-decreasing, so if
            // `over_read_bits()` already exceeds the end-of-tile bound here it is guaranteed
            // to fail `check_tile_read_bounds`).
            if r.over_read_bits() > TILE_OVER_READ_LIMIT_BITS {
                return Err(TileError::CorruptTile);
            }
            row += 8;
        }
        Ok(())
    }

    /// `decode_partition( r, c, bsize )` (spec §6.4.3).
    fn decode_partition(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        bsize: u8,
    ) -> Result<(), TileError> {
        if row >= self.mi_rows || col >= self.mi_cols {
            return Ok(());
        }
        let num8x8 = NUM_8X8_BLOCKS_WIDE_LOOKUP[bsize as usize] as u32;
        let half_block8x8 = num8x8 >> 1;
        let has_rows = (row + half_block8x8) < self.mi_rows;
        let has_cols = (col + half_block8x8) < self.mi_cols;

        let partition = self.read_partition(r, (row, col), bsize, num8x8, has_rows, has_cols);
        let subsize = SUBSIZE_LOOKUP[partition as usize][bsize as usize];
        if subsize == BLOCK_INVALID {
            return Err(TileError::InvalidPartition);
        }

        if subsize < BLOCK_8X8 || partition == PARTITION_NONE {
            self.decode_block(r, row, col, subsize)?;
        } else if partition == PARTITION_HORZ {
            self.decode_block(r, row, col, subsize)?;
            if has_rows {
                self.decode_block(r, row + half_block8x8, col, subsize)?;
            }
        } else if partition == PARTITION_VERT {
            self.decode_block(r, row, col, subsize)?;
            if has_cols {
                self.decode_block(r, row, col + half_block8x8, subsize)?;
            }
        } else {
            debug_assert_eq!(partition, PARTITION_SPLIT);
            self.decode_partition(r, row, col, subsize)?;
            self.decode_partition(r, row, col + half_block8x8, subsize)?;
            self.decode_partition(r, row + half_block8x8, col, subsize)?;
            self.decode_partition(r, row + half_block8x8, col + half_block8x8, subsize)?;
        }

        if bsize == BLOCK_8X8 || partition != PARTITION_SPLIT {
            let above_val = 15u8 >> B_WIDTH_LOG2_LOOKUP[subsize as usize];
            let left_val = 15u8 >> B_HEIGHT_LOG2_LOOKUP[subsize as usize];
            let c0 = col as usize;
            self.above_ctx.partition[c0..c0 + num8x8 as usize].fill(above_val);
            for i in 0..num8x8 {
                self.left_partition_context[((row + i) & 7) as usize] = left_val;
            }
        }
        Ok(())
    }

    /// Reads the `partition` syntax element (spec §9.3.1 / §9.3.2).
    ///
    /// The spec §9.3.2 text states "use kf_partition_probs when FrameIsIntra == 0", but this
    /// is a known erratum. Since `compressed_header()` (spec §6.3) only reads
    /// `partition_probs` when `FrameIsIntra == 0`, implementing the text literally would mean
    /// referencing `partition_probs` while it is uninitialized and never updated for
    /// FrameIsIntra == 1 (key frames), which also contradicts the intent of the `kf_` prefix
    /// (a fixed table specific to key frames). This implementation corrects the wording: for
    /// `FrameIsIntra == 1` (key frames / intra-only frames) it uses the fixed table
    /// `KF_PARTITION_PROBS`, and for `FrameIsIntra == 0` (inter frames) it uses
    /// `partition_probs` as updated by `compressed_header()` (`self.probs.partition_probs`).
    fn read_partition(
        &mut self,
        r: &mut BoolDecoder,
        pos: (u32, u32),
        bsize: u8,
        num8x8: u32,
        has_rows: bool,
        has_cols: bool,
    ) -> u8 {
        let (row, col) = pos;
        let bsl = MI_WIDTH_LOG2_LOOKUP[bsize as usize] as u32;
        let boffset = MI_WIDTH_LOG2_LOOKUP[BLOCK_64X64 as usize] as u32 - bsl;
        let mut above = 0u32;
        let mut left = 0u32;
        for i in 0..num8x8 {
            above |= self.above_ctx.partition[(col + i) as usize] as u32;
            left |= self.left_partition_context[((row + i) % 8) as usize] as u32;
        }
        let above_bit = (above & (1 << boffset)) > 0;
        let left_bit = (left & (1 << boffset)) > 0;
        let ctx = (bsl * 4 + (left_bit as u32) * 2 + (above_bit as u32)) as usize;
        let probs = if self.frame_is_intra {
            &KF_PARTITION_PROBS[ctx]
        } else {
            &self.probs.partition_probs[ctx]
        };

        // Spec §9.3: `partition` is always a type T syntax element, and even when no bit is
        // read due to hasRows/hasCols (the tree selection process returns a value directly),
        // the counting process from spec §9.3.4 is always invoked (`counts_partition[ctx][syntax]`).
        let partition = if has_rows && has_cols {
            r.read_tree(&PARTITION_TREE, |node| probs[node]) as u8
        } else if has_cols {
            // cols_partition_tree: node2 is fixed at 1.
            if r.read_bool(probs[1]) {
                PARTITION_SPLIT
            } else {
                PARTITION_HORZ
            }
        } else if has_rows {
            // rows_partition_tree: node2 is fixed at 2.
            if r.read_bool(probs[2]) {
                PARTITION_SPLIT
            } else {
                PARTITION_VERT
            }
        } else {
            PARTITION_SPLIT
        };
        if !self.frame_is_intra {
            self.count(|c| c.partition[ctx][partition as usize] += 1);
        }
        partition
    }

    /// `decode_block( r, c, subsize )` (spec §6.4.4).
    fn decode_block(
        &mut self,
        r: &mut BoolDecoder,
        row: u32,
        col: u32,
        subsize: u8,
    ) -> Result<(), TileError> {
        let avail_u = row > 0;
        let avail_l = col > self.mi_col_start;

        let (mut info, block_mvs) = if self.frame_is_intra {
            (
                self.intra_frame_mode_info(r, row, col, subsize, avail_u, avail_l)?,
                None,
            )
        } else {
            self.inter_frame_mode_info(r, row, col, subsize, avail_u, avail_l)?
        };
        let is_inter = info.ref_frame0() != INTRA_FRAME;

        // Guard the residual/predict path against malformed bitstreams before entering it: it
        // indexes fixed size/tx tables and unwraps reference views without re-validating, so a
        // corrupted block size or reference would panic there rather than surface as a decode
        // error. A conformant stream never trips these (they reject only values it cannot
        // produce), so this changes no valid-input output. The first two were found by
        // tests/robustness_test; the reference-size bound by review (pinned red->green in
        // tests/synthetic_scaled_ref_test.rs).
        let bsize = info.mi_size().max(BLOCK_8X8);
        if SS_SIZE_LOOKUP[bsize as usize][self.subsampling_x as usize][self.subsampling_y as usize]
            == BLOCK_INVALID
        {
            return Err(TileError::InvalidPartition);
        }
        if is_inter {
            for rf in info.ref_frame() {
                if rf > INTRA_FRAME {
                    match self.resolved_refs[(rf - LAST_FRAME) as usize].as_deref() {
                        None => return Err(TileError::MissingReference),
                        // Spec §8.5.2.3's conformance bounds: 2x upper bound (caps reference-
                        // scaling step at 32, sizing `subpel::MAX_INTERMEDIATE_HEIGHT`), 1/16x
                        // lower bound, and non-zero reference dimensions (defense-in-depth).
                        Some(r)
                            if r.width == 0
                                || r.height == 0
                                || (r.width as u64) > 2 * (self.frame_width as u64)
                                || (r.height as u64) > 2 * (self.frame_height as u64)
                                || 16 * (r.width as u64) < (self.frame_width as u64)
                                || 16 * (r.height as u64) < (self.frame_height as u64) =>
                        {
                            return Err(TileError::RefFrameSizeOutOfRange);
                        }
                        Some(_) => {}
                    }
                }
            }
        }

        let eob_total = self.residual(
            r,
            row,
            col,
            &info,
            block_mvs.as_ref(),
            avail_u,
            avail_l,
            is_inter,
        );
        // Spec §6.4.4: when is_inter && subsize >= BLOCK_8X8 && EobTotal == 0, retroactively
        // set skip to 1 (since it turns out there was actually no residual at all).
        if is_inter && subsize >= BLOCK_8X8 && eob_total == 0 {
            info.set_skip(true);
        }
        if is_inter && subsize < BLOCK_8X8 {
            if let Some(mvs) = block_mvs.as_ref() {
                info.extra = self.mi_grid.push_sub_mvs(mvs);
            }
        }

        let bw = NUM_8X8_BLOCKS_WIDE_LOOKUP[subsize as usize] as u32;
        let bh = NUM_8X8_BLOCKS_HIGH_LOOKUP[subsize as usize] as u32;
        // SAFETY: `decode_partition` keeps every block `[row..row+bh) x [col..col+bw)` aligned
        // inside this tile/strip's superblock-padded `mi_grid` bounds.
        unsafe {
            self.mi_grid.fill_rect_unchecked(row, col, bw, bh, info);
        }
        Ok(())
    }
}
