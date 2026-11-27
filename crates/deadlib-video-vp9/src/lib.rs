//! Dependency-free VP9 decoding of picture packets, IVF and MP4.

#![no_std]

extern crate alloc;

pub mod ivf;

mod bit_reader;
mod bool_coder;
mod common;
mod compressed_header;
mod counts;
mod dpb;
mod framebuffer;
mod header;
mod loop_filter;
mod mp4;
mod mv_ref_tables;
mod predict;
mod prob_tables;
mod quant;
mod scan;
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
mod simd;
mod subpel;
mod superframe;
mod tile;
mod transform;

use alloc::{boxed::Box, vec, vec::Vec};

use alloc::sync::Arc;
use core::fmt;

pub use bool_coder::BoolCoderError;
pub use compressed_header::CompressedHeaderError;
pub use header::HeaderError;
pub use mp4::{decode_mp4, Mp4Error};
pub use tile::TileError;

use compressed_header::{parse_compressed_header, FrameContext, FrameContextStore};
use counts::{adapt_coef_probs, adapt_noncoef_probs};
use dpb::{Dpb, RefFrameData};
use header::{
    parse_uncompressed_header, ColorConfig, FrameHeader, FrameType, LoopFilterDeltas,
    PersistentState, SegFeaturePersist,
};
use tile::{MiGrid, TileDecoder};

/// Error type covering everything that can cause [`Decoder::push_packet`] to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Failed to parse the uncompressed header (`uncompressed_header`). Two variants are
    /// raised by the [`Decoder`] itself rather than the parser: `FrameSizeTooLarge` (a
    /// signaled size the decoder refuses to allocate) and `RefFrameFormatMismatch` (an
    /// inter frame whose reference doesn't share its bit depth / subsampling).
    Header(HeaderError),
    /// Failed to parse the compressed header (`compressed_header`).
    CompressedHeader(CompressedHeaderError),
    /// Failed to decode tiles, mode info, or tokens.
    Tile(TileError),
    /// Frame data is malformed, e.g. `header_size_in_bytes` exceeds the frame data length.
    TruncatedFrame,
    /// The DPB slot referenced by `show_existing_frame` has no frame stored
    /// (does not occur with normal conformance bitstreams).
    MissingReferenceFrame,
}

impl From<HeaderError> for DecodeError {
    fn from(e: HeaderError) -> Self {
        DecodeError::Header(e)
    }
}

impl From<CompressedHeaderError> for DecodeError {
    fn from(e: CompressedHeaderError) -> Self {
        DecodeError::CompressedHeader(e)
    }
}

impl From<TileError> for DecodeError {
    fn from(e: TileError) -> Self {
        DecodeError::Tile(e)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Header(e) => write!(f, "uncompressed header error: {e}"),
            DecodeError::CompressedHeader(e) => write!(f, "compressed header error: {e}"),
            DecodeError::Tile(e) => write!(f, "tile decode error: {e}"),
            DecodeError::TruncatedFrame => f.write_str("truncated VP9 frame data"),
            DecodeError::MissingReferenceFrame => {
                f.write_str("show_existing_frame references an empty DPB slot")
            }
        }
    }
}

impl core::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            DecodeError::Header(e) => Some(e),
            DecodeError::CompressedHeader(e) => Some(e),
            DecodeError::Tile(e) => Some(e),
            DecodeError::TruncatedFrame | DecodeError::MissingReferenceFrame => None,
        }
    }
}

/// One decoded plane's pixel data. 8-bit streams (`BitDepth == 8`) decode into `U8` (no
/// memory bloat from widening the overwhelmingly common case); 10/12-bit streams (VP9
/// profiles 2/3) decode into `U16` (each sample 0..=1023 or 0..=4095).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaneData {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl PlaneData {
    /// Number of samples (not bytes) -- e.g. `width * height` for a cropped plane, the same
    /// meaning regardless of variant.
    pub fn len(&self) -> usize {
        match self {
            PlaneData::U8(v) => v.len(),
            PlaneData::U16(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows the samples as 8-bit. Panics if this plane is `U16` (10/12-bit) -- for
    /// consumers that only ever handle 8-bit output.
    pub fn as_u8(&self) -> &[u8] {
        match self {
            PlaneData::U8(v) => v,
            PlaneData::U16(_) => panic!("PlaneData::as_u8 called on a 16-bit (10/12-bit) plane"),
        }
    }
}

/// One decoded frame. Cropped to the display size (`FrameWidth`/`FrameHeight`), holding the
/// 3 Y/U/V planes as [`PlaneData`] (row-major). Chroma sizing follows
/// `subsampling_x`/`subsampling_y`: 4:2:0 for profiles 0/2; profiles 1/3 also output
/// 4:2:2, 4:4:0, or 4:4:4.
///
/// The `u`/`v` sizes follow the output process of spec §8.9, computed as
/// `((width + subsampling_x) >> subsampling_x) x ((height + subsampling_y) >> subsampling_y)`
/// (for 4:2:0 this is `((width+1)/2) x ((height+1)/2)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub subsampling_x: u32,
    pub subsampling_y: u32,
    pub y: PlaneData,
    pub u: PlaneData,
    pub v: PlaneData,
}

/// Crops one plane to `(w, h)` and narrows to `PlaneData::U8`/`U16` depending on `bit_depth`
/// (8-bit output stays byte-sized; 10/12-bit needs the full `u16` range).
fn plane_to_plane_data(plane: &framebuffer::Plane, w: usize, h: usize, bit_depth: u8) -> PlaneData {
    if bit_depth == 8 {
        PlaneData::U8(plane.crop_u8(w, h))
    } else {
        PlaneData::U16(plane.crop(w, h))
    }
}

/// Crops the frame buffer (`planes`) to the display size and builds a [`Frame`].
fn crop_to_frame(
    planes: &[framebuffer::Plane; 3],
    width: u32,
    height: u32,
    color_config: &ColorConfig,
) -> Frame {
    let sub_x = color_config.subsampling_x as u32;
    let sub_y = color_config.subsampling_y as u32;
    let uv_width = ((width + sub_x) >> sub_x) as usize;
    let uv_height = ((height + sub_y) >> sub_y) as usize;
    let bit_depth = color_config.bit_depth;

    Frame {
        width,
        height,
        bit_depth,
        subsampling_x: sub_x,
        subsampling_y: sub_y,
        y: plane_to_plane_data(&planes[0], width as usize, height as usize, bit_depth),
        u: plane_to_plane_data(&planes[1], uv_width, uv_height, bit_depth),
        v: plane_to_plane_data(&planes[2], uv_width, uv_height, bit_depth),
    }
}

/// Builds a 10/12-bit [`RefFrameData`] for DPB storage by taking ownership of the decoded `planes`
/// and cropping their logical dimensions in-place (zero allocation, zero copy; the backing buffers
/// keep their superblock-aligned stride and are recycled when evicted from the DPB).
fn build_ref_frame_data_u16_in_place(
    mut planes: [framebuffer::Plane; 3],
    width: u32,
    height: u32,
    color_config: &ColorConfig,
) -> RefFrameData {
    let sub_x = color_config.subsampling_x as u32;
    let sub_y = color_config.subsampling_y as u32;
    let uv_width = ((width + sub_x) >> sub_x) as usize;
    let uv_height = ((height + sub_y) >> sub_y) as usize;

    planes[0].crop_in_place(width as usize, height as usize);
    planes[1].crop_in_place(uv_width, uv_height);
    planes[2].crop_in_place(uv_width, uv_height);
    let [y, u, v] = planes;

    RefFrameData {
        width,
        height,
        subsampling_x: sub_x,
        subsampling_y: sub_y,
        bit_depth: color_config.bit_depth,
        y: framebuffer::RefPlane::from_u16_plane(y),
        u: framebuffer::RefPlane::from_u16_plane(u),
        v: framebuffer::RefPlane::from_u16_plane(v),
    }
}

fn ref_frame_data_to_frame(data: &RefFrameData) -> Frame {
    Frame {
        width: data.width,
        height: data.height,
        bit_depth: data.bit_depth,
        subsampling_x: data.subsampling_x,
        subsampling_y: data.subsampling_y,
        y: data.y.to_plane_data(),
        u: data.u.to_plane_data(),
        v: data.v.to_plane_data(),
    }
}

/// Configurable resource limits enforced before allocating per-frame decode buffers.
///
/// Defaults match the crate's historical limits (`max_luma_samples = 1 << 27`, unbounded
/// per-axis `max_width` / `max_height`). Pass a stricter configuration to
/// [`Decoder::with_limits`] to bound worst-case memory usage on untrusted streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderLimits {
    /// Maximum allowed `width * height` (luma samples per frame).
    pub max_luma_samples: u64,
    /// Maximum allowed frame width in pixels.
    pub max_width: u32,
    /// Maximum allowed frame height in pixels.
    pub max_height: u32,
}

impl Default for DecoderLimits {
    fn default() -> Self {
        Self {
            max_luma_samples: header::MAX_FRAME_LUMA_SAMPLES,
            max_width: u32::MAX,
            max_height: u32::MAX,
        }
    }
}

/// A stateful decoder for decoding multiple frames in sequence.
///
/// VP9 carries the following state across frames, so frames cannot be processed
/// independently one by one:
/// - Reference frame slots (`RefFrameWidth`/`RefFrameHeight`, spec §6.2.5
///   `frame_size_with_refs`) and the actual pixel data (`Dpb`, spec §8.10).
/// - Frame contexts (the 4 probability table slots selected by `frame_context_idx`,
///   spec §7.1.2 `load_probs`/`save_probs`).
/// - The previous frame's `Mvs`/`RefFrames`, referenced when `UsePrevFrameMvs`
///   (spec §7.2.6) is true (this implementation keeps the previous frame's
///   mode-info grid in its entirety).
/// - The loop filter's `ref_deltas`/`mode_deltas` (spec §7.2, reset only by
///   `setup_past_independence()`).
/// - `LastFrameType` (spec §7.2; used in the `updateFactor` calculation for
///   probability adaptation, spec §8.4.3).
pub struct Decoder {
    /// Allocation limits enforced before per-frame working buffers are allocated.
    limits: DecoderLimits,
    /// Cross-frame state spec §7.2 requires: `RefFrameWidth`/`RefFrameHeight`, the loop
    /// filter's `ref_deltas`/`mode_deltas`, and the segmentation feature state -- see
    /// [`PersistentState`].
    persist: PersistentState,
    frame_contexts: Box<FrameContextStore>,
    /// Used for computing `UsePrevFrameMvs` (spec §7.2.6). Not updated on
    /// `show_existing_frame` (because `compute_image_size` isn't called).
    prev_frame_dims: Option<(u32, u32)>,
    prev_show_frame: Option<bool>,
    /// The previous frame's `Mvs`/`RefFrames` (equivalent to `PrevMvs`/`PrevRefFrames`).
    /// `Arc`-wrapped so handing a read-only copy to next frame's `TileDecoder` (when
    /// `UsePrevFrameMvs`) is a refcount bump rather than a deep clone.
    prev_mi_grid: Option<Arc<MiGrid>>,
    /// Inter frames and intra-only frames (`Profile == 0`) don't resend
    /// `color_config` in the bitstream, so the value is carried over from the
    /// most recent keyframe/intra-only frame (spec §7.2: it's a conformance
    /// requirement that bit depth and subsampling match the reference frame).
    last_color_config: Option<ColorConfig>,
    /// The actual pixel data of reference frames (spec §8.10 `FrameStore`).
    dpb: Dpb,
    /// `PrevSegmentIds` (spec §6.4.14), row-major `MiRows x MiCols` (unpadded).
    /// Cleared to all-zero on the first frame and whenever the frame size changes
    /// (spec §7.2.6), updated after decoding only when
    /// `segmentation_enabled && segmentation_update_map` (spec §8.1 step 3). `Arc`-wrapped
    /// so the per-frame clone-in to `TileDecoder` (unconditional, unlike `prev_mi_grid`) is
    /// a refcount bump rather than a deep clone.
    prev_segment_ids: Arc<Vec<u8>>,
    /// Explicit `(mi_cols, mi_rows)` dimensions matching `prev_segment_ids`. Stored alongside
    /// the map and committed only when a frame succeeds so a failed frame can never leave the
    /// map length out of sync with `prev_frame_dims`.
    prev_segment_ids_dims: Option<(u32, u32)>,
    /// `LastFrameType` (spec §7.2). Not updated on `show_existing_frame` frames.
    last_frame_type: Option<FrameType>,
    /// Recycled whole-frame `[Plane; 3]` working buffers (and evicted 10/12-bit DPB planes),
    /// at most one per active resolution geometry.
    plane_pool: Vec<[framebuffer::Plane; 3]>,
    /// Recycled cropped `[Vec<u8>; 3]` buffers from evicted 8-bit DPB slots (at most one per
    /// active resolution geometry).
    ref_u8_pool: Vec<[Vec<u8>; 3]>,
    /// Recycled `MiGrid` allocation from the replaced `prev_mi_grid`.
    mi_grid_pool: Option<MiGrid>,
    /// Recycled frame-width above contexts for `TileDecoder`.
    above_ctx_pool: Option<tile::AboveContexts>,
    /// Recycled heap-boxed per-block scratch for `TileDecoder`.
    tile_scratch_pool: Option<Box<tile::TileScratch>>,
    /// Recycled syntax counter table for frames with backward adaptation enabled.
    counts_pool: Option<Box<counts::Counts>>,
}

/// Cap on pooled frame-buffer sets held by [`Decoder`] (at most one per active geometry,
/// covering up to 3-layer spatial SVC without retaining stale resolutions).
const MAX_POOLED_PLANE_SETS: usize = 3;

type PlaneGeom = ((usize, usize), (usize, usize));

#[inline]
fn plane_set_geom(planes: &[framebuffer::Plane; 3]) -> PlaneGeom {
    (planes[0].backing_dims(), planes[1].backing_dims())
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self::with_limits(DecoderLimits::default())
    }

    /// Decodes one VP9 packet in bitstream order.
    ///
    /// Hidden frames update the reference frames without producing a picture.
    /// If a superframe contains more than one shown frame, the last one is returned.
    pub fn push_packet(&mut self, packet: &[u8]) -> Result<Option<Frame>, DecodeError> {
        let mut shown = None;
        for data in superframe::split_superframe(packet) {
            if let Some(frame) = self.decode_one_frame(data)? {
                shown = Some(frame);
            }
        }
        Ok(shown)
    }

    /// Creates a new [`Decoder`] enforcing custom pre-allocation [`DecoderLimits`].
    pub fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            limits,
            persist: PersistentState::default(),
            frame_contexts: Box::new(FrameContextStore::new()),
            prev_frame_dims: None,
            prev_show_frame: None,
            prev_mi_grid: None,
            last_color_config: None,
            dpb: Dpb::new(),
            prev_segment_ids: Arc::new(Vec::new()),
            prev_segment_ids_dims: None,
            last_frame_type: None,
            plane_pool: Vec::new(),
            ref_u8_pool: Vec::new(),
            mi_grid_pool: None,
            above_ctx_pool: None,
            tile_scratch_pool: None,
            counts_pool: None,
        }
    }

    /// Finds and removes a pooled `[Plane; 3]` reusable for `(y_w, y_h, uv_w, uv_h)`.
    /// Prefers an exact backing-dimension match, falling back to capacity reuse bounded by the
    /// largest live DPB resolution (so multi-layer 8-bit SVC shares one working buffer set with
    /// zero reallocations).
    fn take_matching_planes(
        &mut self,
        y_w: usize,
        y_h: usize,
        uv_w: usize,
        uv_h: usize,
    ) -> Option<[framebuffer::Plane; 3]> {
        let want: PlaneGeom = ((y_w, y_w * y_h), (uv_w, uv_w * uv_h));
        if let Some(idx) = self
            .plane_pool
            .iter()
            .position(|p| plane_set_geom(p) == want)
        {
            return Some(self.plane_pool.swap_remove(idx));
        }
        let (max_y, max_uv) = self.dpb.max_sb_plane_elems();
        let max_y = max_y.max(y_w * y_h);
        let max_uv = max_uv.max(uv_w * uv_h);
        for idx in 0..self.plane_pool.len() {
            let p = &mut self.plane_pool[idx];
            if p[0].try_reuse_working(y_w, y_h, max_y)
                && p[1].try_reuse_working(uv_w, uv_h, max_uv)
                && p[2].try_reuse_working(uv_w, uv_h, max_uv)
            {
                return Some(self.plane_pool.swap_remove(idx));
            }
        }
        None
    }

    /// Finds and removes a pooled `[Vec<u8>; 3]` matching `(y_len, uv_len)`.
    fn take_matching_ref_u8(&mut self, y_len: usize, uv_len: usize) -> Option<[Vec<u8>; 3]> {
        let idx = self.ref_u8_pool.iter().position(|v| {
            v[0].capacity() >= y_len
                && v[1].capacity() >= uv_len
                && v[0].len() == y_len
                && v[1].len() == uv_len
        })?;
        Some(self.ref_u8_pool.swap_remove(idx))
    }

    /// Recycles `item` into `pool` (at most one entry per active geometry, pruning stale ones).
    fn recycle_by_geom<T, G: Copy + Eq>(
        pool: &mut Vec<T>,
        item: T,
        active_geom: G,
        geom_of: impl Fn(&T) -> G,
        in_dpb: impl Fn(G) -> bool,
    ) {
        let geom = geom_of(&item);
        pool.retain(|entry| {
            let g = geom_of(entry);
            g == active_geom || in_dpb(g)
        });
        if (geom == active_geom || in_dpb(geom))
            && pool.len() < MAX_POOLED_PLANE_SETS
            && !pool.iter().any(|entry| geom_of(entry) == geom)
        {
            pool.push(item);
        }
    }

    fn recycle_planes(&mut self, planes: [framebuffer::Plane; 3], active_geom: PlaneGeom) {
        let dpb = &self.dpb;
        Self::recycle_by_geom(
            &mut self.plane_pool,
            planes,
            active_geom,
            plane_set_geom,
            |g| dpb.has_u16_geometry(g),
        );
    }

    fn recycle_ref_u8(&mut self, bufs: [Vec<u8>; 3], active_u8_geom: (usize, usize)) {
        let dpb = &self.dpb;
        Self::recycle_by_geom(
            &mut self.ref_u8_pool,
            bufs,
            active_u8_geom,
            |b| (b[0].len(), b[1].len()),
            |g| dpb.has_u8_geometry(g),
        );
    }

    /// Prepares the `PrevSegmentIds` buffer to pass into `TileDecoder` without mutating
    /// persistent decoder state yet (committed only after tile decode succeeds):
    /// - `compute_image_size()` step 1 (spec §7.2.6): first invocation or
    ///   FrameWidth/FrameHeight changed since the previous invocation (checked via both
    ///   `prev_frame_dims` and `prev_segment_ids_dims`).
    /// - `setup_past_independence()` (spec §7.2): `FrameIsIntra || error_resilient_mode`
    ///   (passed as `setup_past_independence`).
    ///
    /// Returns `(map, cleared)` where `cleared` is true when a fresh all-zero map was created.
    fn prepare_prev_segment_ids(
        &self,
        dims: (u32, u32),
        image_size: &header::ImageSize,
        setup_past_independence: bool,
    ) -> (Arc<Vec<u8>>, bool) {
        let mi_dims = (image_size.mi_cols, image_size.mi_rows);
        if setup_past_independence
            || self.prev_frame_dims != Some(dims)
            || self.prev_segment_ids_dims != Some(mi_dims)
        {
            (
                Arc::new(vec![
                    0u8;
                    (image_size.mi_cols * image_size.mi_rows) as usize
                ]),
                true,
            )
        } else {
            (Arc::clone(&self.prev_segment_ids), false)
        }
    }

    /// Spec §8.1 step 3: commits `PrevSegmentIds` on the success path. Refreshes from this
    /// frame's `SegmentIds` (read out of `grid`) when `segmentation_enabled &&
    /// segmentation_update_map`; otherwise persists the cleared zero map if
    /// [`Self::prepare_prev_segment_ids`] reset it for this frame.
    fn commit_prev_segment_ids(
        &mut self,
        header: &header::NewFrameHeader,
        image_size: &header::ImageSize,
        grid: &MiGrid,
        frame_prev_segment_ids: Arc<Vec<u8>>,
        cleared: bool,
    ) {
        let mi_dims = (image_size.mi_cols, image_size.mi_rows);
        if header.segmentation.enabled && header.segmentation.update_map {
            let mut new_map = vec![0u8; (image_size.mi_cols * image_size.mi_rows) as usize];
            for row in 0..image_size.mi_rows {
                for col in 0..image_size.mi_cols {
                    // SAFETY: `row < mi_rows <= grid.rows()` and `col < mi_cols <= grid.cols()`
                    // on this whole-frame owned grid (`col0 == 0`).
                    new_map[(row * image_size.mi_cols + col) as usize] =
                        unsafe { grid.get_unchecked(row, col) }.segment_id();
                }
            }
            self.prev_segment_ids = Arc::new(new_map);
            self.prev_segment_ids_dims = Some(mi_dims);
        } else if cleared {
            self.prev_segment_ids = frame_prev_segment_ids;
            self.prev_segment_ids_dims = Some(mi_dims);
        }
    }

    /// Reference frame update process (spec §8.10) + optional shown-frame construction.
    ///
    /// - For **8-bit** frames (`bit_depth == 8`), narrows the cropped pixels once into packed
    ///   `u8` DPB planes (`RefPlaneData::U8`), immediately returns the working `u16` `[Plane; 3]`
    ///   back to `plane_pool`, and clones the compact `u8` planes if `show_frame` is set (so only
    ///   a single working `u16` buffer is ever needed and DPB slots use half the memory).
    /// - For **10/12-bit** frames, crops the `u16` planes in-place into the DPB with zero copy
    ///   and recycles evicted slots back into `plane_pool`.
    fn output_and_refresh_refs(
        &mut self,
        header: &header::NewFrameHeader,
        color_config: &ColorConfig,
        planes: [framebuffer::Plane; 3],
    ) -> Option<Frame> {
        let active_geom = plane_set_geom(&planes);
        let sub_x = color_config.subsampling_x as u32;
        let sub_y = color_config.subsampling_y as u32;
        let y_w = header.width as usize;
        let y_h = header.height as usize;
        let uv_w = ((header.width + sub_x) >> sub_x) as usize;
        let uv_h = ((header.height + sub_y) >> sub_y) as usize;

        if header.refresh_frame_flags == 0 {
            let frame = header
                .show_frame
                .then(|| crop_to_frame(&planes, header.width, header.height, color_config));
            self.recycle_planes(planes, active_geom);
            return frame;
        }

        let (frame, ref_data) = if color_config.bit_depth == 8 {
            let mut bufs = self
                .take_matching_ref_u8(y_w * y_h, uv_w * uv_h)
                .unwrap_or_else(|| [Vec::new(), Vec::new(), Vec::new()]);
            planes[0].narrow_u8_into(y_w, y_h, &mut bufs[0]);
            planes[1].narrow_u8_into(uv_w, uv_h, &mut bufs[1]);
            planes[2].narrow_u8_into(uv_w, uv_h, &mut bufs[2]);
            let [y_u8, u_u8, v_u8] = bufs;
            let frame = header.show_frame.then(|| Frame {
                width: header.width,
                height: header.height,
                bit_depth: 8,
                subsampling_x: sub_x,
                subsampling_y: sub_y,
                y: PlaneData::U8(y_u8.clone()),
                u: PlaneData::U8(u_u8.clone()),
                v: PlaneData::U8(v_u8.clone()),
            });
            self.recycle_planes(planes, active_geom);
            let ref_data = Arc::new(RefFrameData {
                width: header.width,
                height: header.height,
                subsampling_x: sub_x,
                subsampling_y: sub_y,
                bit_depth: 8,
                y: framebuffer::RefPlane::from_u8_cropped(y_w, y_h, y_u8),
                u: framebuffer::RefPlane::from_u8_cropped(uv_w, uv_h, u_u8),
                v: framebuffer::RefPlane::from_u8_cropped(uv_w, uv_h, v_u8),
            });
            (frame, ref_data)
        } else {
            let frame = header
                .show_frame
                .then(|| crop_to_frame(&planes, header.width, header.height, color_config));
            let ref_data = Arc::new(build_ref_frame_data_u16_in_place(
                planes,
                header.width,
                header.height,
                color_config,
            ));
            (frame, ref_data)
        };

        let active_u8_geom = (y_w * y_h, uv_w * uv_h);
        let mut evicted = Vec::new();
        self.dpb
            .update_and_recycle(header.refresh_frame_flags, &ref_data, |old| {
                evicted.push(old)
            });
        for old in evicted {
            match (
                old.y.into_backing(),
                old.u.into_backing(),
                old.v.into_backing(),
            ) {
                (Ok(y), Ok(u), Ok(v)) => self.recycle_ref_u8([y, u, v], active_u8_geom),
                (Err(y), Err(u), Err(v)) => self.recycle_planes([y, u, v], active_geom),
                _ => {}
            }
        }
        // Prune stale geometries and oversized working-plane capacities after the DPB update.
        let dpb = &self.dpb;
        let (max_y, max_uv) = dpb.max_sb_plane_elems();
        let max_y = max_y.max(active_geom.0 .1);
        let max_uv = max_uv.max(active_geom.1 .1);
        self.plane_pool.retain(|p| {
            let g = plane_set_geom(p);
            (g == active_geom || dpb.has_u16_geometry(g))
                && p[0].capacity() <= max_y
                && p[1].capacity() <= max_uv
                && p[2].capacity() <= max_uv
        });
        self.ref_u8_pool.retain(|b| {
            let g = (b[0].len(), b[1].len());
            g == active_u8_geom || dpb.has_u8_geometry(g)
        });
        for (slot, size) in self.persist.ref_frame_sizes.iter_mut().enumerate() {
            if (header.refresh_frame_flags >> slot) & 1 == 1 {
                *size = (header.width, header.height);
            }
        }
        frame
    }

    /// Resolves this frame's effective [`ColorConfig`] without mutating decoder state.
    ///
    /// `header.color_config` is `Some` on key frames and intra-only frames; inter frames inherit
    /// the most recent intra frame's config (falling back to a neutral 8-bit 4:2:0 placeholder if
    /// an inter frame arrives before any intra frame). `last_color_config` is updated only after
    /// the frame decodes successfully (see [`Self::update_persistent_state`]).
    fn resolve_color_config(&self, header: &header::NewFrameHeader) -> ColorConfig {
        header.color_config.unwrap_or_else(|| {
            self.last_color_config
                .unwrap_or_else(ColorConfig::fallback_inter)
        })
    }

    /// Computes which stored frame context slot(s) `setup_past_independence()` (spec §7.2) will
    /// reset when this frame succeeds.
    fn pending_frame_context_reset(header: &header::NewFrameHeader) -> FrameContextReset {
        if header.frame_is_intra || header.error_resilient_mode {
            frame_context_reset(
                header.frame_type,
                header.error_resilient_mode,
                header.reset_frame_context,
                header.frame_context_idx,
            )
        } else {
            FrameContextReset::None
        }
    }

    /// Applies a deferred [`FrameContextReset`] on the success path.
    fn apply_frame_context_reset(&mut self, reset: FrameContextReset) {
        match reset {
            FrameContextReset::All => self.frame_contexts.reset_all(),
            FrameContextReset::Slot(idx) => {
                self.frame_contexts.save(idx, FrameContext::default());
            }
            FrameContextReset::None => {}
        }
    }

    /// Resolves the 3 DPB reference slots for an inter frame (cheap `Arc` bumps) and validates
    /// that every present reference matches this frame's bit depth and chroma subsampling
    /// (spec §8.5.1 / libvpx `valid_ref_frame_img_fmt`).
    ///
    /// Reference *sizes* are deliberately validated at block use time in `decode_block` rather
    /// than here, because a conformant stream (e.g. 3-layer spatial SVC) may list an out-of-range
    /// slot it never predicts from.
    fn resolve_and_validate_refs(
        &self,
        header: &header::NewFrameHeader,
        color_config: &ColorConfig,
    ) -> Result<[Option<Arc<RefFrameData>>; 3], DecodeError> {
        let resolved_refs: [Option<Arc<RefFrameData>>; 3] = if header.frame_is_intra {
            [None, None, None]
        } else {
            core::array::from_fn(|i| self.dpb.get_arc(header.ref_frame_idx[i]))
        };
        for r in resolved_refs.iter().flatten() {
            if r.bit_depth != color_config.bit_depth
                || r.subsampling_x != color_config.subsampling_x as u32
                || r.subsampling_y != color_config.subsampling_y as u32
            {
                return Err(DecodeError::Header(HeaderError::RefFrameFormatMismatch));
            }
        }
        Ok(resolved_refs)
    }

    /// Updates cross-frame persistent state (dimensions, show flag, color config, loop-filter
    /// deltas, segmentation feature persistence, and `LastFrameType`) after a newly decoded frame
    /// succeeds.
    fn update_persistent_state(
        &mut self,
        header: &header::NewFrameHeader,
        color_config: ColorConfig,
    ) {
        // compute_image_size is never called for show_existing_frame, so by the time we
        // reach here it has always been called (spec §7.2.6). Recorded for UsePrevFrameMvs.
        self.prev_frame_dims = Some((header.width, header.height));
        self.prev_show_frame = Some(header.show_frame);
        if header.frame_is_intra {
            self.last_color_config = Some(color_config);
        }
        self.persist.loop_filter_deltas = LoopFilterDeltas {
            ref_deltas: header.loop_filter.ref_deltas,
            mode_deltas: header.loop_filter.mode_deltas,
        };
        self.persist.segmentation = SegFeaturePersist {
            enabled: header.segmentation.feature_enabled,
            data: header.segmentation.feature_data,
            abs_or_delta: header.segmentation.abs_or_delta_update,
        };
        self.last_frame_type = Some(header.frame_type);
    }

    /// Stores `mi_grid` as `prev_mi_grid` and recycles the replaced allocation when unshared.
    fn store_and_recycle_mi_grid(&mut self, mut mi_grid: MiGrid) {
        mi_grid.clear_sub_mvs();
        if let Some(old_grid) = self.prev_mi_grid.replace(Arc::new(mi_grid)) {
            if let Ok(owned) = Arc::try_unwrap(old_grid) {
                self.mi_grid_pool = Some(owned);
            }
        }
    }

    fn decode_one_frame(&mut self, frame_data: &[u8]) -> Result<Option<Frame>, DecodeError> {
        let (parsed, consumed) = { parse_uncompressed_header(frame_data, &self.persist)? };
        let header = match parsed {
            FrameHeader::New(h) => h,
            FrameHeader::ShowExistingFrame {
                frame_to_show_map_idx,
            } => {
                let data = self
                    .dpb
                    .get(frame_to_show_map_idx)
                    .ok_or(DecodeError::MissingReferenceFrame)?;
                return Ok(Some(ref_frame_data_to_frame(data)));
            }
        };

        // Unconditionally reject zero dimensions before any arithmetic or allocation.
        if header.width == 0 || header.height == 0 {
            return Err(DecodeError::Header(HeaderError::InvalidFrameSize));
        }

        // Reject an absurd or over-limit frame size before it reaches the buffer allocations
        // below (which use `vec![]` and would abort the process on failure, not return an error).
        if header.width > self.limits.max_width
            || header.height > self.limits.max_height
            || header.width as u64 * header.height as u64 > self.limits.max_luma_samples
        {
            return Err(DecodeError::Header(HeaderError::FrameSizeTooLarge));
        }

        let color_config = self.resolve_color_config(&header);

        let compressed_start = consumed;
        let compressed_end = compressed_start
            .checked_add(header.header_size_in_bytes as usize)
            .ok_or(DecodeError::TruncatedFrame)?;
        if compressed_end > frame_data.len() {
            return Err(DecodeError::TruncatedFrame);
        }
        let compressed_bytes = &frame_data[compressed_start..compressed_end];
        let tile_data = &frame_data[compressed_end..];

        // UsePrevFrameMvs (spec §7.2.6).
        let use_prev_frame_mvs = !header.frame_is_intra
            && !header.error_resilient_mode
            && self.prev_frame_dims == Some((header.width, header.height))
            && self.prev_show_frame == Some(true);

        // Compute any context reset without mutating `self.frame_contexts` until the frame
        // succeeds, while seeding `parse_compressed_header` with the exact post-reset slot.
        let ctx_idx = header.effective_frame_context_idx();
        let ctx_reset = Self::pending_frame_context_reset(&header);
        let starting_probs = match ctx_reset {
            FrameContextReset::All => FrameContext::default(),
            FrameContextReset::Slot(idx) if idx == ctx_idx => FrameContext::default(),
            _ => self.frame_contexts.load(ctx_idx),
        };
        let compressed = { parse_compressed_header(compressed_bytes, &header, starting_probs)? };

        let resolved_refs = self.resolve_and_validate_refs(&header, &color_config)?;

        // Cheap pre-allocation structural check on tile headers/sizes/marker bits so tiny
        // truncated packets fail before allocating multi-megabyte working planes or `MiGrid`.
        tile::validate_tile_layout(tile_data, header.tile_cols_log2, header.tile_rows_log2)?;

        let image_size = header::compute_image_size(header.width, header.height);
        let (frame_prev_segment_ids, seg_ids_cleared) = self.prepare_prev_segment_ids(
            (header.width, header.height),
            &image_size,
            header.frame_is_intra || header.error_resilient_mode,
        );

        let prev_grid = if use_prev_frame_mvs {
            self.prev_mi_grid.clone()
        } else {
            None
        };
        let y_w = (image_size.sb64_cols * 64) as usize;
        let y_h = (image_size.sb64_rows * 64) as usize;
        let uv_w = y_w >> color_config.subsampling_x;
        let uv_h = y_h >> color_config.subsampling_y;
        let recycled = tile::RecycledBuffers {
            planes: self.take_matching_planes(y_w, y_h, uv_w, uv_h),
            mi_grid: self.mi_grid_pool.take(),
            above_ctx: self.above_ctx_pool.take(),
            scratch: self.tile_scratch_pool.take(),
            counts: self.counts_pool.take(),
        };
        let backward_adapt = header.refresh_frame_context
            && !header.error_resilient_mode
            && !header.frame_parallel_decoding_mode;
        let mut tile_decoder = TileDecoder::new_with_buffers(
            &header,
            color_config,
            &compressed,
            use_prev_frame_mvs,
            prev_grid,
            resolved_refs,
            Arc::clone(&frame_prev_segment_ids),
            backward_adapt,
            recycled,
        );
        tile_decoder.decode_tiles(tile_data)?;
        // Spec §8.1 step 2: "If loop_filter_level is not equal to 0, the loop filter
        // process ... is invoked" -- gated on the frame-level value, not per-edge deltas.
        if header.loop_filter.level != 0 {
            tile_decoder.apply_loop_filter(&header.loop_filter);
        }
        self.commit_prev_segment_ids(
            &header,
            &image_size,
            tile_decoder.mi_grid(),
            frame_prev_segment_ids,
            seg_ids_cleared,
        );
        self.apply_frame_context_reset(ctx_reset);

        // Backward adaptation (spec §6.1.2) needs `tile_decoder.counts()` before `tile_decoder`
        // is consumed; FPM / error-resilient refresh saves `compressed.probs` directly after
        // `tile_decoder.into_parts()` drops its `Arc` so `Arc::unwrap_or_clone` avoids cloning.
        if backward_adapt {
            let starting_probs = self.frame_contexts.load(ctx_idx);
            let final_probs = refresh_probs(
                &header,
                starting_probs,
                &compressed,
                tile_decoder.counts(),
                self.last_frame_type,
            );
            self.frame_contexts.save(ctx_idx, final_probs);
        }

        self.update_persistent_state(&header, color_config);

        // Last use of tile_decoder: consume it to drop its read-only `Arc`s to DPB slots,
        // `prev_mi_grid`, and `compressed.probs` BEFORE refreshing DPB / saving probs /
        // replacing `self.prev_mi_grid`, so evicted buffers have strong count 1 and recycle
        // cleanly into our pools.
        let (planes, mi_grid, above_ctx, scratch, counts) = tile_decoder.into_parts();
        self.above_ctx_pool = Some(above_ctx);
        self.tile_scratch_pool = Some(scratch);
        if let Some(c) = counts {
            self.counts_pool = Some(c);
        }
        if header.refresh_frame_context && !backward_adapt {
            self.frame_contexts
                .save(ctx_idx, Arc::unwrap_or_clone(compressed.probs));
        }
        let frame = self.output_and_refresh_refs(&header, &color_config, planes);
        self.store_and_recycle_mi_grid(mi_grid);
        Ok(frame)
    }
}

/// Which stored frame context slot(s) get reset to defaults, per the `save_probs`
/// calls inside `setup_past_independence()` (spec §7.2). Only meaningful when called
/// under `FrameIsIntra || error_resilient_mode` (the condition under which
/// `setup_past_independence()` runs at all; see the call site in `push_packet`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameContextReset {
    /// `reset_frame_context` is 0 or 1: no stored context is touched.
    None,
    /// `reset_frame_context == 2`: only the slot at this (raw, pre-`setup_past_independence`) index is reset.
    Slot(u8),
    /// `frame_type == KEY_FRAME`, `error_resilient_mode`, or `reset_frame_context == 3`: all 4 slots are reset.
    All,
}

/// `frame_context_idx` must be the raw bitstream value (`NewFrameHeader::frame_context_idx`, not
/// `effective_frame_context_idx()`), since `save_probs( frame_context_idx )` runs before
/// `setup_past_independence()` forces the effective index to 0.
fn frame_context_reset(
    frame_type: FrameType,
    error_resilient_mode: bool,
    reset_frame_context: u8,
    frame_context_idx: u8,
) -> FrameContextReset {
    if frame_type == FrameType::KeyFrame || error_resilient_mode || reset_frame_context == 3 {
        FrameContextReset::All
    } else if reset_frame_context == 2 {
        FrameContextReset::Slot(frame_context_idx)
    } else {
        FrameContextReset::None
    }
}

/// `refresh_probs()` (spec §6.1.2): the frame context to save back after a frame's decode --
/// a pure function of the frame's header, the pre-forward-update starting probabilities
/// (`load_probs`' value), the parsed compressed header, the tile decode's adaptation counts,
/// and the previous frame's type (spec §8.4.3's updateFactor). Backward adaptation runs only
/// when neither error_resilient_mode nor frame_parallel_decoding_mode is set; otherwise the
/// post-forward-update probabilities are saved as-is.
fn refresh_probs(
    header: &header::NewFrameHeader,
    starting_probs: FrameContext,
    compressed: &compressed_header::CompressedHeader,
    counts: &counts::Counts,
    last_frame_type: Option<FrameType>,
) -> FrameContext {
    if !header.error_resilient_mode && !header.frame_parallel_decoding_mode {
        // load_probs( frame_context_idx ): restore all tables except tx_probs/skip_prob
        // to their pre-forward-update value (starting_probs). tx_probs/skip_prob are
        // left at their post-forward-update value from compressed_header(). The two
        // fields are copied aside (24B + 3B) before moving starting_probs into working,
        // rather than cloning the whole multi-KB struct just to overwrite most of it.
        let pre_update_tx_probs = starting_probs.tx_probs;
        let pre_update_skip_prob = starting_probs.skip_prob;
        let mut working = starting_probs;
        working.tx_probs = compressed.probs.tx_probs;
        working.skip_prob = compressed.probs.skip_prob;

        // Spec §8.4.3: determining updateFactor.
        let update_factor = if header.frame_is_intra {
            112
        } else if last_frame_type == Some(FrameType::KeyFrame) {
            128
        } else {
            112
        };
        adapt_coef_probs(&mut working.coef_probs, counts, update_factor);

        if !header.frame_is_intra {
            // load_probs2( frame_context_idx ): also restore tx_probs/skip_prob to
            // their pre-forward-update value before applying adapt_noncoef_probs.
            working.tx_probs = pre_update_tx_probs;
            working.skip_prob = pre_update_skip_prob;
            adapt_noncoef_probs(
                &mut working,
                counts,
                header.interpolation_filter,
                compressed.tx_mode,
                header.allow_high_precision_mv,
            );
        }
        working
    } else {
        (*compressed.probs).clone()
    }
}
