use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::bitstream::BitstreamReader;
use crate::deblock::{self, MbInfo, MbType};
use crate::decode_cabac::CabacMbResult;
#[cfg(feature = "surface")]
use crate::dpb::SurfacePool;
use crate::dpb::{
    DecodedPicture, Dpb, Motion, PictureMeta, PictureStructure, PictureWriter, ReferenceKeys,
    ReferenceStatus,
};
use crate::error::DecodeError;
use crate::mv_pred::{is_long_term, ImplicitWeights, WeightContext};
use crate::nal::{NalUnit, NalUnitType};
use crate::pixel::Pixel;
use crate::pps::{parse_pps, Pps};
use crate::slice::{parse_slice_header, SliceHeader, SliceType, MMCO_DIFF_BITS};
use crate::slice_context::{SliceContext, SliceParams};
use crate::sps::{parse_sps, Sps};
#[cfg(feature = "surface")]
use crate::surface::SurfaceStorage;

/// Chroma sampling of a [`Frame`]'s U and V planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChromaFormat {
    /// 4:2:0: chroma planes are half width and half height. Monochrome
    /// (4:0:0) streams also decode to this, with neutral U and V samples.
    #[default]
    Yuv420,
    /// 4:2:2: chroma planes are half width and full height.
    Yuv422,
}

impl ChromaFormat {
    /// Format of the chroma planes for `chroma_format_idc` (4:0:0 outputs
    /// mid-grey 4:2:0 planes).
    pub(crate) fn from_idc(chroma_format_idc: u32) -> Self {
        if chroma_format_idc == 2 {
            ChromaFormat::Yuv422
        } else {
            ChromaFormat::Yuv420
        }
    }

    /// log2 of the luma rows per chroma row: 1 for 4:2:0, 0 for 4:2:2.
    pub(crate) fn shift_y(self) -> u32 {
        match self {
            ChromaFormat::Yuv420 => 1,
            ChromaFormat::Yuv422 => 0,
        }
    }
}

/// A decoded YUV frame, 4:2:0 or 4:2:2 (see [`Frame::chroma_format`]).
///
/// The Y plane contains `width × height` samples. The U and V planes each
/// contain [`chroma_width`](Frame::chroma_width) ×
/// [`chroma_height`](Frame::chroma_height) samples. Planes are tightly packed
/// and row-major. The [`bytes_per_sample`](Frame::bytes_per_sample) method
/// gives the storage size of each sample. Monochrome streams decode to 4:2:0
/// with neutral chroma.
///
/// Frames returned by [`Decoder::decode_nal`] are emitted in **decode order**,
/// not display order. To present them in the intended order, sort by
/// `pic_order_cnt` within each GOP. See the crate-level documentation for the
/// recommended pattern (and a common pitfall around IDR boundary handling).
#[derive(Debug, Clone)]
pub struct Frame {
    /// Display width in luma samples.
    pub width: u32,
    /// Display height in luma samples.
    pub height: u32,
    /// Luma plane, `width * height` samples, row-major.
    pub y: Vec<u8>,
    /// Cb chroma plane, `chroma_width() * chroma_height()` samples, row-major.
    pub u: Vec<u8>,
    /// Cr chroma plane, `chroma_width() * chroma_height()` samples, row-major.
    pub v: Vec<u8>,
    /// Picture order count — the H.264 spec's display ordering value.
    /// Use this to sort frames into display order within a GOP.
    pub pic_order_cnt: i32,
    /// Chroma sampling of `u` and `v`.
    pub chroma_format: ChromaFormat,
    /// Bits per sample, luma and chroma: 8, or 10. At 10 bits each
    /// sample takes two bytes of `y`/`u`/`v`, little-endian (FFmpeg's
    /// `yuv420p10le` / `yuv422p10le` layout); see [`Frame::sample`].
    pub bit_depth: u8,
}

impl Frame {
    /// Bytes per sample in `y`, `u` and `v`: 1, or 2 above 8 bits.
    pub fn bytes_per_sample(&self) -> usize {
        if self.bit_depth > 8 {
            2
        } else {
            1
        }
    }

    /// Sample `i` (in samples, row-major) of a plane, whatever the bit
    /// depth.
    pub fn sample(&self, plane: &[u8], i: usize) -> u16 {
        if self.bit_depth > 8 {
            u16::from_le_bytes([plane[2 * i], plane[2 * i + 1]])
        } else {
            plane[i] as u16
        }
    }

    /// Width of the U and V planes.
    pub fn chroma_width(&self) -> u32 {
        self.width / 2
    }

    /// Height of the U and V planes.
    pub fn chroma_height(&self) -> u32 {
        self.height >> self.chroma_format.shift_y()
    }
}

/// In-progress picture state shared across slices within the same frame.
#[derive(Clone)]
pub(crate) struct PictureState {
    pub(crate) frame: Frame,
    pub(crate) frame_num: u32,
    pub(crate) poc: i32,
    /// Top and bottom field POCs (see `Dpb::frame_field_pocs`)
    pub(crate) field_poc: [i32; 2],
    pub(crate) nal_unit_type: NalUnitType,
    pub(crate) nal_ref_idc: u8,
    scratch: PictureScratch,
    /// Current slice ID counter (incremented for each new slice).
    pub(crate) current_slice_id: u16,
    #[allow(dead_code)]
    pub(crate) prev_mb_qp: i32,
    #[allow(dead_code)]
    pub(crate) last_qp_delta_nonzero: bool,
    // Slice header info for finalization
    pub(crate) mmco_ops: Vec<(u32, u32)>,
    pub(crate) adaptive_ref_pic_marking: bool,
    pub(crate) long_term_reference_flag: bool,
    pub(crate) is_intra_slice: bool,
    pub(crate) chroma_qp_index_offset: i32,
    pub(crate) second_chroma_qp_index_offset: i32,
    pub(crate) mb_width: u32,
    pub(crate) mb_height: u32,
    /// True if this picture uses MBAFF (mb_adaptive_frame_field_flag && !field_pic_flag).
    pub(crate) mbaff_frame_flag: bool,
    /// True if this picture is a field picture (field_pic_flag=1).
    pub(crate) field_pic_flag: bool,
    /// True if this is the bottom field (only valid when field_pic_flag=true).
    pub(crate) bottom_field_flag: bool,
    /// Full frame height (needed for field picture output combining).
    pub(crate) frame_height: u32,
    /// MaxFrameNum from the SPS, for the picture numbers MMCO commands give.
    pub(crate) max_frame_num: u32,
    /// Top-left of the display window in this picture's coded planes, in luma
    /// samples (field lines for field pictures).
    pub(crate) crop_x: u32,
    pub(crate) crop_y: u32,
}

/// Reusable macroblock storage. Motion arrays leave with reference pictures.
#[derive(Clone, Default)]
pub(crate) struct PictureScratch {
    // Per-MB arrays that persist across slices
    nc_luma: Vec<u8>,
    nc_cb: Vec<u8>,
    nc_cr: Vec<u8>,
    mv_store_l0: Vec<[i16; 2]>,
    mv_store_l1: Vec<[i16; 2]>,
    ref_idx_store_l0: Vec<i8>,
    reference_keys: Vec<ReferenceKeys>,
    ref_idx_store_l1: Vec<i8>,
    mvd_store: Vec<[i16; 2]>,
    mvd_store_l1: Vec<[i16; 2]>,
    mb_info: Vec<deblock::MbInfo>,
    i4x4_modes: Vec<u8>,
    // CABAC neighbor context state
    mb_cbp: Vec<u16>,
    mb_chroma_pred: Vec<u8>,
    mb_is_8x8dct: Vec<bool>,
    mb_skip: Vec<bool>,
    mb_is_direct: Vec<bool>,
    blk_is_direct: Vec<bool>,
    is_i16x16: Vec<bool>,
    /// Per-MB slice ID for slice boundary detection. MBs from different
    /// slices are treated as unavailable for CABAC context and MV prediction.
    mb_slice_id: Vec<u16>,
    /// Deblocking settings of each slice so far, indexed by slice ID
    /// (each MB is filtered as its own slice says, spec 8.7).
    slice_deblock: Vec<deblock::SliceDeblock>,
    /// Per-MB-pair field decoding flag (MBAFF only). Indexed by pair address.
    mb_field_decoding: Vec<bool>,
}

impl PictureScratch {
    fn use_motion(&mut self, motion: Motion) {
        self.mv_store_l0 = motion.mv_l0;
        self.mv_store_l1 = motion.mv_l1;
        self.ref_idx_store_l0 = motion.ref_idx_l0;
        self.ref_idx_store_l1 = motion.ref_idx_l1;
        self.mb_slice_id = motion.slice_ids;
        self.reference_keys = motion.reference_keys;
    }

    fn reset(&mut self, total_mbs: usize) {
        // Release the previous picture's storage when the coded area changes.
        if self.mb_info.len() != total_mbs {
            *self = Self::default();
        }
        fn reset<T: Clone>(values: &mut Vec<T>, len: usize, value: T) {
            values.clear();
            values.resize(len, value);
        }

        let blocks = total_mbs * 16;
        reset(&mut self.nc_luma, blocks, 0);
        reset(&mut self.nc_cb, total_mbs * 8, 0);
        reset(&mut self.nc_cr, total_mbs * 8, 0);
        reset(&mut self.mv_store_l0, blocks, [0; 2]);
        reset(&mut self.mv_store_l1, blocks, [0; 2]);
        reset(&mut self.ref_idx_store_l0, blocks, -1);
        self.reference_keys.clear();
        reset(&mut self.ref_idx_store_l1, blocks, -1);
        reset(&mut self.mvd_store, blocks, [0; 2]);
        reset(&mut self.mvd_store_l1, blocks, [0; 2]);
        self.mb_info
            .resize_with(total_mbs, deblock::MbInfo::default);
        for info in &mut self.mb_info {
            info.reset_metadata();
        }
        reset(&mut self.i4x4_modes, blocks, 2);
        reset(&mut self.mb_cbp, total_mbs, 0);
        reset(&mut self.mb_chroma_pred, total_mbs, 0);
        reset(&mut self.mb_is_8x8dct, total_mbs, false);
        reset(&mut self.mb_skip, total_mbs, false);
        reset(&mut self.mb_is_direct, total_mbs, false);
        reset(&mut self.blk_is_direct, blocks, false);
        reset(&mut self.is_i16x16, total_mbs, false);
        reset(&mut self.mb_slice_id, total_mbs, 0);
        reset(&mut self.mb_field_decoding, total_mbs.div_ceil(2), false);
        self.slice_deblock.clear();
    }
}

/// Streaming H.264 decoder.
///
/// Feed NAL units one at a time with [`decode_nal`](Self::decode_nal) and
/// receive decoded frames as they become available. Call [`flush`](Self::flush)
/// at end-of-stream to retrieve any final buffered frame.
///
/// Internally maintains parameter set tables (SPS, PPS), a Decoded Picture
/// Buffer (DPB), and any in-progress slice state.
///
/// # Example
///
/// ```no_run
/// use deadlib_video_h264::decoder::Decoder;
/// use deadlib_video_h264::nal::parse_annex_b;
///
/// let bitstream = std::fs::read("input.h264").unwrap();
/// let nals = parse_annex_b(&bitstream);
/// let mut decoder = Decoder::new();
///
/// for nal in &nals {
///     if let Ok(Some(frame)) = decoder.decode_nal(nal) {
///         println!("Decoded {}x{} frame, POC={}",
///                  frame.width, frame.height, frame.pic_order_cnt);
///     }
/// }
/// if let Some(frame) = decoder.flush() {
///     println!("Final frame, POC={}", frame.pic_order_cnt);
/// }
/// ```
pub struct Decoder {
    sps_table: BTreeMap<u32, Arc<Sps>>,
    pps_table: BTreeMap<u32, Arc<Pps>>,
    dpb: Dpb,
    /// In-progress picture being assembled from one or more slices.
    pending: Option<PictureState>,
    scratch: PictureScratch,
    recycled_frame: Option<Frame>,
    #[cfg(feature = "surface")]
    surface_pool: Option<Arc<SurfacePool>>,
    /// First field of a field-picture pair awaiting its complement for output.
    pending_field: Option<Frame>,
    /// The last frame returned came from a picture with MMCO 5, which starts
    /// a new output period (see [`OrderedDecoder`]).
    period_start: bool,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    /// Create a new decoder with empty parameter set tables and DPB.
    pub fn new() -> Self {
        Self {
            sps_table: BTreeMap::new(),
            pps_table: BTreeMap::new(),
            dpb: Dpb::new(0).recycle_pictures(),
            pending: None,
            scratch: PictureScratch::default(),
            recycled_frame: None,
            #[cfg(feature = "surface")]
            surface_pool: None,
            pending_field: None,
            period_start: false,
        }
    }

    /// Return a consumed frame so its plane allocations can be reused.
    ///
    /// The decoder retains at most one returned frame and reuses it when
    /// the next picture has compatible geometry and sample format.
    pub fn recycle_frame(&mut self, frame: Frame) {
        self.recycled_frame = Some(frame);
    }

    /// Feed a single NAL unit to the decoder.
    ///
    /// Returns:
    /// - `Ok(Some(frame))` — a decoded frame is ready (in **decode order**;
    ///   sort by `pic_order_cnt` for display order). Note that the returned
    ///   frame belongs to the *previous* picture: when this NAL starts a new
    ///   picture, the decoder finalizes the previous one and returns it.
    /// - `Ok(None)` — the NAL was consumed but no frame is ready yet
    ///   (e.g., SPS, PPS, SEI, or the first slice of a multi-slice picture).
    /// - `Err(e)` — the NAL was malformed or used an unsupported feature.
    ///
    /// Call [`flush`](Self::flush) after the last NAL to retrieve any
    /// remaining buffered frame.
    pub fn decode_nal(&mut self, nal: &NalUnit) -> Result<Option<Frame>, DecodeError> {
        self.decode_nal_as(nal, Self::finalize_pending)
    }

    pub(crate) fn decode_nal_as<T>(
        &mut self,
        nal: &NalUnit,
        finalize: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Result<Option<T>, DecodeError> {
        match nal.nal_unit_type {
            NalUnitType::Sps | NalUnitType::Pps => {
                store_parameter_set(&mut self.sps_table, &mut self.pps_table, &mut self.dpb, nal)?;
                Ok(None)
            }
            NalUnitType::Sei => Ok(None),
            NalUnitType::SliceIdr | NalUnitType::Slice => {
                if is_redundant_slice(&self.sps_table, &self.pps_table, nal) {
                    return Ok(None);
                }
                // Peek at first_mb_in_slice to detect new vs continuation slice
                let mut peek = BitstreamReader::new(&nal.rbsp);
                let first_mb = peek.read_ue().unwrap_or(0);

                // Check if this is a new picture: first_mb==0 means first
                // slice of a new picture. Continuation slices (first_mb > 0)
                // belong to the same picture even for IDR NALs.
                let is_new_picture = first_mb == 0;

                // Finalize pending frame if a new picture starts
                let prev_frame = if is_new_picture { finalize(self) } else { None };

                // Decode this slice (creates or continues PictureState).
                // For CAVLC multi-slice, end-of-slice detection may fail,
                // causing errors from reading past the slice boundary. If
                // we had a pending picture, the already-decoded MBs are
                // valid, so we treat the error as end-of-slice.
                //
                // Since decode_slice takes self.pending via take(), we must
                // save a backup for continuation slices so we can restore it
                // if the decode fails mid-slice.
                let had_pending = !is_new_picture && self.pending.is_some();
                let pending_backup = if had_pending {
                    self.pending.clone()
                } else {
                    None
                };
                match self.decode_slice(nal) {
                    Ok(()) => {}
                    Err(_e) if self.pending.is_some() => {}
                    Err(_e) if had_pending => {
                        // decode_slice consumed self.pending but failed before
                        // reassembling it. Restore the backup so already-decoded
                        // MBs from earlier slices are preserved.
                        self.pending = pending_backup;
                    }
                    Err(e) => return Err(e),
                }

                Ok(prev_frame)
            }
            _ => Ok(None),
        }
    }

    /// Finalize any pending picture and return it. Call this after the last
    /// NAL has been fed via [`decode_nal`](Self::decode_nal) to retrieve the
    /// final frame, which is otherwise held internally awaiting a new picture
    /// to trigger its release.
    pub fn flush(&mut self) -> Option<Frame> {
        let frame = self.finalize_pending();
        if frame.is_some() {
            return frame;
        }
        // If a single field is pending (complement never arrived), output it
        // as a half-height frame rather than losing it.
        self.pending_field.take()
    }

    /// Frame rate from the most recently parsed SPS's VUI timing info, as
    /// `(numerator, denominator)`. Returns `None` if no SPS has been parsed
    /// yet, if the SPS does not contain VUI timing info, or if the values
    /// are zero.
    ///
    /// For example, a stream encoded at 29.97 fps would return
    /// `Some((60000, 2002))`. Use [`frame_rate_f64`](Self::frame_rate_f64)
    /// for a single floating-point value.
    pub fn frame_rate(&self) -> Option<(u32, u32)> {
        // Return the first SPS that has timing info. For typical streams
        // there's only one SPS, so this is unambiguous.
        self.sps_table.values().find_map(|s| s.frame_rate())
    }

    /// Frame rate as a single floating-point value. Convenience wrapper
    /// around [`frame_rate`](Self::frame_rate).
    pub fn frame_rate_f64(&self) -> Option<f64> {
        let (n, d) = self.frame_rate()?;
        Some(n as f64 / d as f64)
    }

    /// Finalize the pending picture: apply deblocking, insert into DPB, return frame.
    fn finalize_pending(&mut self) -> Option<Frame> {
        let mut ps = self.pending.take()?;
        deblock_picture(&mut ps);
        if ps.nal_ref_idc != 0 || ps.nal_unit_type == NalUnitType::SliceIdr {
            let pic = make_decoded_picture(&mut ps, self.dpb.take_spare_picture());
            apply_reference_marking(
                &mut self.dpb,
                pic,
                ps.nal_unit_type,
                ps.nal_ref_idc,
                ps.frame_num,
                ps.max_frame_num,
                ps.field_pic_flag,
                &ps.mmco_ops,
                ps.adaptive_ref_pic_marking,
                ps.long_term_reference_flag,
            );
        }
        let mmco5 = has_mmco5(&ps.mmco_ops);
        self.scratch = core::mem::take(&mut ps.scratch);
        let frame = finish_output(ps, &mut self.pending_field);
        self.period_start = mmco5 && frame.is_some();
        frame
    }

    #[cfg(feature = "surface")]
    pub(crate) fn finalize_surface(&mut self) -> Option<SurfaceStorage> {
        let pending = self.pending.as_ref()?;
        if pending.field_pic_flag
            || (pending.nal_ref_idc == 0 && pending.nal_unit_type != NalUnitType::SliceIdr)
        {
            return self.finalize_pending().map(SurfaceStorage::Owned);
        }
        let mut ps = self.pending.take().unwrap();
        deblock_picture(&mut ps);
        let meta = picture_meta(&ps);
        let motion = take_motion(&mut ps);
        let pic = Arc::new(DecodedPicture::pooled(
            meta,
            [
                core::mem::take(&mut ps.frame.y),
                core::mem::take(&mut ps.frame.u),
                core::mem::take(&mut ps.frame.v),
            ],
            motion,
            Arc::downgrade(self.surface_pool()),
        ));
        apply_reference_marking(
            &mut self.dpb,
            pic.clone(),
            ps.nal_unit_type,
            ps.nal_ref_idc,
            ps.frame_num,
            ps.max_frame_num,
            ps.field_pic_flag,
            &ps.mmco_ops,
            ps.adaptive_ref_pic_marking,
            ps.long_term_reference_flag,
        );
        self.scratch = core::mem::take(&mut ps.scratch);
        let mmco5 = has_mmco5(&ps.mmco_ops);
        if mmco5 {
            ps.frame.pic_order_cnt = 0;
        }
        let visible =
            ps.crop_x + ps.frame.width <= meta.width && ps.crop_y + ps.frame.height <= meta.height;
        self.period_start = mmco5 && visible;
        visible.then_some(SurfaceStorage::Reference {
            display: ps.frame,
            picture: pic,
            coded_width: meta.width as usize,
            origin: (ps.crop_x, ps.crop_y),
        })
    }

    #[cfg(feature = "surface")]
    pub(crate) fn flush_surface(&mut self) -> Option<SurfaceStorage> {
        self.finalize_surface()
            .or_else(|| self.pending_field.take().map(SurfaceStorage::Owned))
    }

    #[cfg(feature = "surface")]
    pub(crate) fn with_surface_pool() -> Self {
        Self {
            surface_pool: Some(Arc::new(SurfacePool::default())),
            ..Self::new()
        }
    }

    #[cfg(feature = "surface")]
    pub(crate) fn surface_pool(&self) -> &Arc<SurfacePool> {
        self.surface_pool
            .as_ref()
            .expect("surface decoder has a pool")
    }

    /// Whether the frame just returned starts a new output period (its
    /// picture had MMCO 5), clearing the flag.
    pub(crate) fn take_period_start(&mut self) -> bool {
        core::mem::take(&mut self.period_start)
    }

    fn decode_slice(&mut self, nal: &NalUnit) -> Result<(), DecodeError> {
        let plan = prepare_slice(&self.sps_table, &self.pps_table, &mut self.dpb, nal)?;
        let pending = self.pending.take();
        if pending.is_none()
            && self.scratch.mb_info.len() == plan.total_mbs
            && self.scratch.mv_store_l0.capacity() == 0
        {
            let motion = self.dpb.take_spare_motion(plan.total_mbs * 16);
            #[cfg(feature = "surface")]
            let motion =
                motion.or_else(|| self.surface_pool.as_ref()?.take_motion(plan.total_mbs * 16));
            if let Some(motion) = motion {
                self.scratch.use_motion(motion);
            }
        }
        #[cfg(feature = "surface")]
        if let Some(pool) = self
            .surface_pool
            .as_ref()
            .filter(|_| pending.is_none() && self.recycled_frame.is_none())
        {
            let bit_depth = 8 + plan.sps.bit_depth_luma_minus8 as u8;
            let bps = if bit_depth > 8 { 2 } else { 1 };
            let chroma_format = ChromaFormat::from_idc(plan.sps.chroma_format_idc);
            let y_len = (plan.coded_width * plan.coded_height) as usize * bps;
            let c_len = y_len >> (1 + chroma_format.shift_y());
            let lengths = [y_len, c_len, c_len];
            if let Some([y, u, v]) = pool.take_picture(lengths) {
                self.recycled_frame = Some(Frame {
                    width: plan.width,
                    height: plan.height,
                    y,
                    u,
                    v,
                    pic_order_cnt: plan.current_poc,
                    chroma_format,
                    bit_depth,
                });
            }
        }
        let ps = run_slice(
            &plan,
            &nal.rbsp,
            pending,
            &mut self.scratch,
            &mut self.recycled_frame,
            None,
        )?;
        self.pending = Some(ps);
        Ok(())
    }
}

/// Reject sequences this decoder cannot decode correctly. Only 8- and
/// 10-bit 4:2:0, 4:2:2 and 4:0:0 with the normal transform path are
/// implemented; anything else would decode
/// to wrong pictures instead of failing. Checked when a slice activates its
/// SPS (not when the SPS is parsed), so streams that merely carry such an
/// SPS still decode, and [`parse_sps_info`](crate::nal::parse_sps_info)
/// can still describe it.
pub(crate) fn check_supported(sps: &Sps) -> Result<(), DecodeError> {
    match sps.chroma_format_idc {
        0..=2 => {}
        3 => return Err(DecodeError::Unsupported("4:4:4 chroma")),
        _ => return Err(DecodeError::InvalidSyntax("chroma_format_idc out of range")),
    }
    // 8 and 10 bits; chroma (when present) at the luma depth, as FFmpeg
    if !matches!(sps.bit_depth_luma_minus8, 0 | 2) {
        return Err(DecodeError::Unsupported("bit depth other than 8 or 10"));
    }
    if sps.chroma_format_idc != 0 && sps.bit_depth_chroma_minus8 != sps.bit_depth_luma_minus8 {
        return Err(DecodeError::Unsupported(
            "different luma and chroma bit depths",
        ));
    }
    if sps.qpprime_y_zero_transform_bypass_flag {
        return Err(DecodeError::Unsupported(
            "lossless (transform bypass) coding",
        ));
    }
    Ok(())
}

/// Parse an SPS or PPS NAL and store it in the parameter-set tables. An SPS
/// also updates the DPB's reference frame capacity.
pub(crate) fn store_parameter_set(
    sps_table: &mut BTreeMap<u32, Arc<Sps>>,
    pps_table: &mut BTreeMap<u32, Arc<Pps>>,
    dpb: &mut Dpb,
    nal: &NalUnit,
) -> Result<(), DecodeError> {
    match nal.nal_unit_type {
        NalUnitType::Sps => {
            let sps = parse_sps(&nal.rbsp)?;
            dpb.set_max_ref_frames(sps.max_num_ref_frames);
            sps_table.insert(sps.seq_parameter_set_id, Arc::new(sps));
        }
        NalUnitType::Pps => {
            let pps_id_sps = {
                // Peek at seq_parameter_set_id to find the right SPS
                let mut peek = BitstreamReader::new(&nal.rbsp);
                let _ = peek.read_ue(); // pic_parameter_set_id
                peek.read_ue().ok()
            };
            let sps_ref = pps_id_sps.and_then(|id| sps_table.get(&id)).map(|s| &**s);
            let pps = parse_pps(&nal.rbsp, sps_ref)?;
            pps_table.insert(pps.pic_parameter_set_id, Arc::new(pps));
        }
        _ => {}
    }
    Ok(())
}

/// Everything about one slice that can be decided from its header and the
/// DPB state alone: the parsed header, parameter sets, reference lists and
/// weights and picture geometry. Building a plan
/// is cheap and mutates only the DPB's POC state; running it does the
/// macroblock decode. The split lets the threaded decoder plan slices in
/// coded order on one thread and run them on others.
pub(crate) struct SlicePlan {
    pub(crate) sps: Arc<Sps>,
    pub(crate) pps: Arc<Pps>,
    pub(crate) header: SliceHeader,
    pub(crate) nal_unit_type: NalUnitType,
    pub(crate) nal_ref_idc: u8,
    /// The byte and bit position where slice data starts in the RBSP.
    pub(crate) header_end: (usize, u8),
    pub(crate) current_poc: i32,
    pub(crate) ref_pic_list: Vec<Arc<DecodedPicture>>,
    pub(crate) ref_pic_list_l0: Vec<Arc<DecodedPicture>>,
    pub(crate) ref_pic_list_l1: Vec<Arc<DecodedPicture>>,
    /// Which entries of the B-slice lists are long-term references
    pub(crate) long_term_l0: u32,
    pub(crate) long_term_l1: u32,
    pub(crate) use_weight: u8,
    pub(crate) implicit_weights: ImplicitWeights,
    /// MBAFF implicit weights for field MBs, per MB parity (0 = top), indexed
    /// by field ref_idx: `[parity][l0][l1]` = L0 weight
    pub(crate) implicit_weights_field: [ImplicitWeights; 2],
    /// Top and bottom field POCs of the current picture
    pub(crate) field_poc: [i32; 2],
    pub(crate) slice_qp: i32,
    pub(crate) width: u32,
    pub(crate) frame_height: u32,
    pub(crate) height: u32,
    pub(crate) mb_width: u32,
    pub(crate) mb_height: u32,
    pub(crate) coded_width: u32,
    pub(crate) coded_height: u32,
    pub(crate) total_mbs: usize,
    /// Display window origin in this picture's coded planes (luma samples;
    /// field lines for field pictures).
    pub(crate) crop_x: u32,
    pub(crate) crop_y: u32,
}

/// Header-level half of slice decoding (see [`SlicePlan`]).
/// True for a slice of a redundant coded picture (redundant_pic_cnt > 0),
/// which is skipped as FFmpeg does: the primary picture carries the same
/// content. Checked before the picture boundary, since a redundant
/// picture's first slice would otherwise end the primary one.
pub(crate) fn is_redundant_slice(
    sps_table: &BTreeMap<u32, Arc<Sps>>,
    pps_table: &BTreeMap<u32, Arc<Pps>>,
    nal: &NalUnit,
) -> bool {
    let mut peek = BitstreamReader::new(&nal.rbsp);
    let pps_id = (|| {
        peek.read_ue()?;
        peek.read_ue()?;
        peek.read_ue()
    })();
    let Some(pps) = pps_id.ok().and_then(|id| pps_table.get(&id)) else {
        return false;
    };
    if !pps.redundant_pic_cnt_present_flag {
        return false;
    }
    let Some(sps) = sps_table.get(&pps.seq_parameter_set_id) else {
        return false;
    };
    parse_slice_header(&nal.rbsp, sps, pps, nal.nal_unit_type, nal.nal_ref_idc)
        .is_ok_and(|(header, _)| header.redundant_pic_cnt > 0)
}

pub(crate) fn prepare_slice(
    sps_table: &BTreeMap<u32, Arc<Sps>>,
    pps_table: &BTreeMap<u32, Arc<Pps>>,
    dpb: &mut Dpb,
    nal: &NalUnit,
) -> Result<SlicePlan, DecodeError> {
    // The slice header starts with first_mb_in_slice, slice_type and
    // pic_parameter_set_id; the rest of it depends on that PPS
    let pps_id = {
        let mut peek = BitstreamReader::new(&nal.rbsp);
        peek.read_ue()?;
        peek.read_ue()?;
        peek.read_ue()?
    };
    let pps = pps_table
        .get(&pps_id)
        .ok_or(DecodeError::InvalidSyntax("no PPS available"))?;
    let sps = sps_table
        .get(&pps.seq_parameter_set_id)
        .ok_or(DecodeError::InvalidSyntax("no SPS available"))?;
    check_supported(sps)?;

    let (header, reader) =
        parse_slice_header(&nal.rbsp, sps, pps, nal.nal_unit_type, nal.nal_ref_idc)?;

    if header.slice_type != SliceType::I
        && header.slice_type != SliceType::P
        && header.slice_type != SliceType::B
    {
        return Err(DecodeError::from("unsupported slice type"));
    }
    let is_field_pic = header.field_pic_flag;
    let is_p_slice = header.slice_type == SliceType::P;
    let is_b_slice = header.slice_type == SliceType::B;

    // Compute POC for current picture (needed for B-slice ref list construction)
    let current_poc = dpb.compute_poc(sps, &header, nal.nal_unit_type, nal.nal_ref_idc);

    // Build reference picture lists
    // For field pictures: MaxPicNum = 2*MaxFrameNum, CurrPicNum = 2*frame_num+1
    let max_frame_num = 1u32 << (sps.log2_max_frame_num_minus4 + 4).min(31);
    let max_pic_num = if is_field_pic {
        max_frame_num.saturating_mul(2)
    } else {
        max_frame_num
    };
    let curr_pic_num = if is_field_pic {
        header.frame_num.saturating_mul(2).saturating_add(1)
    } else {
        header.frame_num
    };
    // Skipped frame_num values become "non-existing" reference frames
    // (spec 8.2.5.2); later slices of the picture find no gap left
    if nal.nal_unit_type != NalUnitType::SliceIdr {
        dpb.fill_frame_num_gap(header.frame_num, max_frame_num);
    }
    let mut ref_pic_list = if is_p_slice {
        let mut refs = dpb.short_term_ref_list(
            header.frame_num,
            max_frame_num,
            is_field_pic,
            header.bottom_field_flag,
        );
        // Pad ref list if shorter than num_ref_idx_l0_active (spec 8.2.4.2.1:
        // if the list is shorter, duplicate the last entry to fill)
        if !refs.is_empty() {
            while refs.len() < header.num_ref_idx_l0_active as usize {
                refs.push(refs.last().unwrap().clone());
            }
        }
        refs
    } else {
        vec![]
    };
    let [mut _ref_pic_list_l0, mut _ref_pic_list_l1] = if is_b_slice {
        let mut lists = dpb.b_ref_lists(current_poc, is_field_pic, header.bottom_field_flag);
        let active = [header.num_ref_idx_l0_active, header.num_ref_idx_l1_active];
        for (refs, active) in lists.iter_mut().zip(active) {
            if let Some(last) = refs.last() {
                let active = active as usize;
                if refs.len() < active {
                    refs.resize(active, Arc::clone(last));
                }
            }
        }
        lists
    } else {
        [Vec::new(), Vec::new()]
    };

    // The initial lists keep only their first num_ref_idx_lX_active entries
    // (spec 8.2.4.2); modification works on lists of that length.
    ref_pic_list.truncate(header.num_ref_idx_l0_active as usize);
    _ref_pic_list_l0.truncate(header.num_ref_idx_l0_active as usize);
    _ref_pic_list_l1.truncate(header.num_ref_idx_l1_active as usize);

    // Apply ref_pic_list_modification (spec 8.2.4.3)
    // A command may name any short-term reference, not only one in the list.
    let has_modifications = ((is_p_slice || is_b_slice) && !header.ref_list_mod_l0.is_empty())
        || (is_b_slice && !header.ref_list_mod_l1.is_empty());
    let short_term = if has_modifications {
        dpb.short_term_refs(is_field_pic)
    } else {
        Vec::new()
    };
    let long_term = dpb.long_term_refs(is_field_pic);
    if is_p_slice && !header.ref_list_mod_l0.is_empty() {
        Dpb::apply_ref_list_modification(
            &mut ref_pic_list,
            &header.ref_list_mod_l0,
            curr_pic_num,
            max_pic_num,
            is_field_pic,
            header.bottom_field_flag,
            &short_term,
            &long_term,
        );
    }
    if is_b_slice && !header.ref_list_mod_l0.is_empty() {
        Dpb::apply_ref_list_modification(
            &mut _ref_pic_list_l0,
            &header.ref_list_mod_l0,
            curr_pic_num,
            max_pic_num,
            is_field_pic,
            header.bottom_field_flag,
            &short_term,
            &long_term,
        );
    }
    if is_b_slice && !header.ref_list_mod_l1.is_empty() {
        Dpb::apply_ref_list_modification(
            &mut _ref_pic_list_l1,
            &header.ref_list_mod_l1,
            curr_pic_num,
            max_pic_num,
            is_field_pic,
            header.bottom_field_flag,
            &short_term,
            &long_term,
        );
    }

    // Slice header validation limits each active list to 32 entries.
    let long_term_mask = |list: &[Arc<DecodedPicture>]| {
        let mut mask = 0u32;
        for (i, p) in list.iter().enumerate() {
            if long_term.iter().any(|(q, _)| Arc::ptr_eq(p, q)) {
                mask |= 1 << i;
            }
        }
        mask
    };
    let long_term_l0 = long_term_mask(&_ref_pic_list_l0);
    let long_term_l1 = long_term_mask(&_ref_pic_list_l1);

    // Weighted prediction mode:
    // 0 = no weighting (default)
    // 1 = explicit weights (P-slice weighted_pred_flag=1, or B-slice weighted_bipred_idc=1)
    // 2 = implicit weights (B-slice weighted_bipred_idc=2)
    let use_weight =
        if (is_p_slice && pps.weighted_pred_flag) || (is_b_slice && pps.weighted_bipred_idc == 1) {
            1
        } else if is_b_slice && pps.weighted_bipred_idc == 2 {
            2
        } else {
            0
        };

    // Implicit weighted prediction: compute L0 weight from POC distances.
    // implicit_weights[l0_idx][l1_idx] = w0. L1 weight = 64 - w0. Fixed log2_denom=5.
    let implicit_weight = |cur: i32, poc0: i32, poc1: i32| {
        let td = (poc1 - poc0).clamp(-128, 127);
        if td == 0 {
            32
        } else {
            let tb = (cur - poc0).clamp(-128, 127);
            let tx = (16384 + (td.abs() / 2)) / td;
            let w1 = (tb * tx + 32) >> 8;
            if !(-64..=128).contains(&w1) {
                32
            } else {
                64 - w1
            }
        }
    };
    let implicit_weights = if use_weight == 2 {
        // A long-term reference gets the default weight (spec 8.4.2.3.1)
        ImplicitWeights::new(_ref_pic_list_l0.len(), _ref_pic_list_l1.len(), |k0, k1| {
            if is_long_term(long_term_l0, k0) || is_long_term(long_term_l1, k1) {
                32
            } else {
                implicit_weight(
                    current_poc,
                    _ref_pic_list_l0[k0].pic_order_cnt,
                    _ref_pic_list_l1[k1].pic_order_cnt,
                )
            }
        })
    } else {
        ImplicitWeights::default()
    };
    let field_poc = Dpb::frame_field_pocs(sps, &header, current_poc);
    // MBAFF field MBs predict from fields: field ref_idx k of an MB of parity
    // p is the field of parity p ^ (k & 1) of frame entry k / 2, and the
    // implicit weights use field POCs (spec 8.4.2.3.1).
    let implicit_weights_field = if use_weight == 2 && header.mbaff_frame_flag {
        let field_ref_poc =
            |list: &[Arc<DecodedPicture>], p: usize, k: usize| list[k / 2].field_poc[p ^ (k & 1)];
        [0, 1].map(|p| {
            ImplicitWeights::new(
                _ref_pic_list_l0.len() * 2,
                _ref_pic_list_l1.len() * 2,
                |k0, k1| {
                    if is_long_term(long_term_l0, k0 / 2) || is_long_term(long_term_l1, k1 / 2) {
                        return 32;
                    }
                    implicit_weight(
                        field_poc[p],
                        field_ref_poc(&_ref_pic_list_l0, p, k0),
                        field_ref_poc(&_ref_pic_list_l1, p, k1),
                    )
                },
            )
        })
    } else {
        Default::default()
    };

    // The coded size comes from the SPS macroblock counts (spec 7.4.2.1.1);
    // the display size is that minus the cropping, which may remove a
    // macroblock or more, so it cannot be used to derive the MB counts.
    let width = sps.width();
    let frame_height = sps.height();
    let mb_width = sps.pic_width_in_mbs_minus1.saturating_add(1);
    let map_units = sps.pic_height_in_map_units_minus1.saturating_add(1);
    let frame_mb_height = if sps.frame_mbs_only_flag {
        map_units
    } else {
        map_units.saturating_mul(2)
    };
    // Reject absurd dimensions that would cause allocation overflow, and
    // cropping that removes the whole picture (the display size saturates to
    // 0). H.264 Level 6.2 max is 8192x4320; allow up to 16384x16384.
    if width == 0 || frame_height == 0 || mb_width > 16384 / 16 || frame_mb_height > 16384 / 16 {
        return Err(DecodeError::InvalidSyntax("SPS dimensions out of range"));
    }
    // For field pictures, each field has half the frame height
    let (height, mb_height) = if is_field_pic {
        (frame_height / 2, frame_mb_height / 2)
    } else {
        (frame_height, frame_mb_height)
    };
    let coded_width = mb_width * 16;
    let coded_height = mb_height * 16;
    let total_mbs = (mb_width * mb_height) as usize;
    // Display window origin. The top offset is in frame lines, so a field
    // picture starts half as many of its own lines down.
    let crop_x = sps.crop_left();
    let crop_y = if is_field_pic {
        sps.crop_top() / 2
    } else {
        sps.crop_top()
    };

    let slice_qp = header.qp_y(pps);

    Ok(SlicePlan {
        sps: Arc::clone(sps),
        pps: Arc::clone(pps),
        header_end: reader.position(),
        header,
        nal_unit_type: nal.nal_unit_type,
        nal_ref_idc: nal.nal_ref_idc,
        current_poc,
        ref_pic_list,
        ref_pic_list_l0: _ref_pic_list_l0,
        ref_pic_list_l1: _ref_pic_list_l1,
        long_term_l0,
        long_term_l1,
        use_weight,
        implicit_weights,
        implicit_weights_field,
        field_poc,
        slice_qp,
        width,
        frame_height,
        height,
        mb_width,
        mb_height,
        coded_width,
        coded_height,
        total_mbs,
        crop_x,
        crop_y,
    })
}

/// Macroblock-decode half of slice decoding: creates or continues the
/// picture's [`PictureState`] (`pending` is the state left by an earlier
/// slice of the same picture) and decodes all macroblocks of the slice.
/// Returns the updated state; deblocking and DPB insertion happen when the
/// picture is finalized.
pub(crate) fn run_slice(
    plan: &SlicePlan,
    rbsp: &[u8],
    pending: Option<PictureState>,
    scratch: &mut PictureScratch,
    recycled_frame: &mut Option<Frame>,
    sync: Option<&mut RowSync>,
) -> Result<PictureState, DecodeError> {
    let high_bit_depth = plan.sps.bit_depth_luma_minus8 == 2;
    let mbaff = plan.header.mbaff_frame_flag;
    match (high_bit_depth, mbaff) {
        (false, false) => {
            run_slice_px::<u8, false>(plan, rbsp, pending, scratch, recycled_frame, sync)
        }
        (false, true) => {
            run_slice_px::<u8, true>(plan, rbsp, pending, scratch, recycled_frame, sync)
        }
        (true, false) => {
            run_slice_px::<u16, false>(plan, rbsp, pending, scratch, recycled_frame, sync)
        }
        (true, true) => {
            run_slice_px::<u16, true>(plan, rbsp, pending, scratch, recycled_frame, sync)
        }
    }
}

/// Slice decode specialized by sample depth and macroblock-pair layout.
fn run_slice_px<P: Pixel, const MBAFF: bool>(
    plan: &SlicePlan,
    rbsp: &[u8],
    mut pending: Option<PictureState>,
    spare: &mut PictureScratch,
    recycled_frame: &mut Option<Frame>,
    mut sync: Option<&mut RowSync>,
) -> Result<PictureState, DecodeError> {
    // Covers everything before the MB loop: PictureState allocation and
    // entropy decoder init.
    let header = &plan.header;
    let sps: &Sps = &plan.sps;
    let pps: &Pps = &plan.pps;
    let mut reader = BitstreamReader::new(rbsp);
    reader.set_position(plan.header_end);
    let is_field_pic = header.field_pic_flag;
    let is_p_slice = header.slice_type == SliceType::P;
    let is_b_slice = header.slice_type == SliceType::B;
    let current_poc = plan.current_poc;
    let ref_pic_list: &[Arc<DecodedPicture>] = &plan.ref_pic_list;
    let _ref_pic_list_l0: &[Arc<DecodedPicture>] = &plan.ref_pic_list_l0;
    let _ref_pic_list_l1: &[Arc<DecodedPicture>] = &plan.ref_pic_list_l1;
    let use_weight = plan.use_weight;
    let implicit_weights = &plan.implicit_weights;
    let implicit_weights_field = &plan.implicit_weights_field;
    let frame_height = plan.frame_height;
    let (mb_width, mb_height) = (plan.mb_width, plan.mb_height);
    let (coded_width, coded_height) = (plan.coded_width, plan.coded_height);
    let total_mbs = plan.total_mbs;
    let slice_qp = plan.slice_qp;
    let wctx = WeightContext {
        use_weight,
        wt: header.weight_table.as_ref(),
        implicit_weights,
        implicit_weights_field,
    };

    // Create or reuse PictureState for multi-slice support.
    // For continuation slices (first_mb > 0), reuse the pending state
    // so per-MB data from earlier slices is visible for MV prediction,
    // CABAC neighbor contexts, and deblocking.
    let is_continuation = header.first_mb_in_slice > 0
        && pending.is_some()
        && pending.as_ref().unwrap().scratch.mb_slice_id.len() == total_mbs;
    let ps = if is_continuation {
        pending.take().unwrap()
    } else {
        let mut scratch = core::mem::take(spare);
        scratch.reset(total_mbs);
        PictureState {
            frame: new_picture_frame::<P>(plan, recycled_frame.take()),
            frame_num: header.frame_num,
            poc: current_poc,
            field_poc: plan.field_poc,
            nal_unit_type: plan.nal_unit_type,
            nal_ref_idc: plan.nal_ref_idc,
            scratch,
            current_slice_id: 0,
            prev_mb_qp: slice_qp,
            last_qp_delta_nonzero: false,
            mmco_ops: header.mmco_ops.clone(),
            adaptive_ref_pic_marking: header.adaptive_ref_pic_marking,
            long_term_reference_flag: header.long_term_reference_flag,
            is_intra_slice: header.slice_type == SliceType::I,
            chroma_qp_index_offset: pps.chroma_qp_index_offset,
            second_chroma_qp_index_offset: pps.second_chroma_qp_index_offset,
            mb_width,
            mb_height,
            mbaff_frame_flag: header.mbaff_frame_flag,
            field_pic_flag: header.field_pic_flag,
            bottom_field_flag: header.bottom_field_flag,
            frame_height,
            max_frame_num: 1u32 << (sps.log2_max_frame_num_minus4 + 4).min(31),
            crop_x: plan.crop_x,
            crop_y: plan.crop_y,
        }
    };

    // Destructure into local variables so existing code works unchanged
    let PictureState {
        mut frame,
        frame_num: _ps_frame_num,
        poc: _ps_poc,
        field_poc: ps_field_poc,
        nal_unit_type: _ps_nal_type,
        nal_ref_idc: _ps_nal_ref_idc,
        scratch:
            PictureScratch {
                mut nc_luma,
                mut nc_cb,
                mut nc_cr,
                mut mv_store_l0,
                mut mv_store_l1,
                mut ref_idx_store_l0,
                mut reference_keys,
                mut ref_idx_store_l1,
                mut mvd_store,
                mut mvd_store_l1,
                mut mb_info,
                mut i4x4_modes,
                mut mb_cbp,
                mut mb_chroma_pred,
                mut mb_is_8x8dct,
                mut mb_skip,
                mut mb_is_direct,
                mut blk_is_direct,
                mut is_i16x16,
                mut mb_slice_id,
                mb_field_decoding: mut _mb_field_decoding,
                mut slice_deblock,
            },
        mut current_slice_id,
        prev_mb_qp: _,
        last_qp_delta_nonzero: _,
        mmco_ops: _ps_mmco_ops,
        adaptive_ref_pic_marking: _,
        long_term_reference_flag: _ps_lt_ref_flag,
        is_intra_slice: _ps_is_intra,
        chroma_qp_index_offset: ps_chroma_qp_offset,
        second_chroma_qp_index_offset: ps_second_chroma_qp_offset,
        mb_width: _ps_mb_width,
        mb_height: _ps_mb_height,
        mbaff_frame_flag: _ps_mbaff,
        field_pic_flag: _ps_field,
        bottom_field_flag: _ps_bottom,
        frame_height: _ps_frame_height,
        max_frame_num,
        crop_x,
        crop_y,
    } = ps;

    // Increment slice ID for continuation slices so boundary checks work
    if is_continuation {
        current_slice_id += 1;
    }
    let this_slice_id = current_slice_id;

    // This slice's deblocking settings, at its ID
    let this_slice_deblock = deblock::SliceDeblock {
        disable_deblocking_filter_idc: header.disable_deblocking_filter_idc,
        slice_alpha_c0_offset_div2: header.slice_alpha_c0_offset_div2,
        slice_beta_offset_div2: header.slice_beta_offset_div2,
    };
    slice_deblock.resize(usize::from(this_slice_id), this_slice_deblock);
    slice_deblock.push(this_slice_deblock);

    // CABAC or CAVLC?
    let use_cabac = pps.entropy_coding_mode_flag;

    // Initialize CABAC engine if needed
    let cabac_byte_pos = if use_cabac {
        let (pos, _data) = reader.cabac_start();
        Some(pos)
    } else {
        None
    };
    // Create CabacReader from original RBSP data (avoids borrow conflict with reader)
    let mut cabac_reader = cabac_byte_pos.map(|pos| crate::cabac::CabacReader::new(rbsp, pos));
    let mut cabac_state = if use_cabac {
        crate::cabac::init_cabac_states(
            slice_qp,
            header.slice_type == SliceType::I,
            header.cabac_init_idc,
        )
    } else {
        [0u8; 1024]
    };

    let mut mb_skip_run: i32 = -1; // -1 = not initialized for P slices
    let mut mbaff_skip_lookahead: Option<bool> = None;

    let stride = coded_width as usize;
    // One context owns the macroblock state throughout the slice.
    let mut ctx = SliceContext::<P, MBAFF> {
        _pixel: core::marker::PhantomData::<P>,
        frame: &mut frame,
        stride,
        width: coded_width,
        height: coded_height,
        mb_width,
        nc_luma: &mut nc_luma,
        nc_cb: &mut nc_cb,
        nc_cr: &mut nc_cr,
        mv_store_l0: &mut mv_store_l0,
        mv_store_l1: &mut mv_store_l1,
        ref_idx_store_l0: &mut ref_idx_store_l0,
        ref_idx_store_l1: &mut ref_idx_store_l1,
        mvd_store: &mut mvd_store,
        mvd_store_l1: &mut mvd_store_l1,
        mb_info: &mut mb_info,
        i4x4_modes: &mut i4x4_modes,
        mb_cbp: &mut mb_cbp,
        mb_chroma_pred: &mut mb_chroma_pred,
        mb_is_8x8dct: &mut mb_is_8x8dct,
        mb_skip: &mut mb_skip,
        mbaff_skip_lookahead: &mut mbaff_skip_lookahead,
        mb_is_direct: &mut mb_is_direct,
        blk_is_direct: &mut blk_is_direct,
        is_i16x16: &mut is_i16x16,
        mb_slice_id: &mut mb_slice_id,
        this_slice_id,
        prev_mb_qp: slice_qp,
        last_qp_delta_nonzero: false,
        mb_field_decoding: &mut _mb_field_decoding,
        ly_stride: stride,
        ly_offset: 0,
        lc_stride: (coded_width / 2) as usize,
        lc_offset: 0,
        field_pic_flag: is_field_pic,
        bottom_field_flag: header.bottom_field_flag,
    };

    let params = SliceParams {
        is_p_slice,
        is_b_slice,
        use_weight,
        current_poc,
        current_field_poc: plan.field_poc,
        direct_spatial_mv_pred_flag: header.direct_spatial_mv_pred_flag,
        direct_8x8_inference_flag: sps.direct_8x8_inference_flag,
        transform_8x8_mode_flag: pps.transform_8x8_mode_flag,
        scaling_list_4x4: &pps.scaling_list_4x4,
        scaling_list_8x8: &pps.scaling_list_8x8,
        constrained_intra_pred_flag: pps.constrained_intra_pred_flag,
        chroma_qp_index_offset: pps.chroma_qp_index_offset,
        second_chroma_qp_index_offset: pps.second_chroma_qp_index_offset,
        monochrome: sps.chroma_format_idc == 0,
        ref_pic_list,
        ref_pic_list_l0: _ref_pic_list_l0,
        ref_pic_list_l1: _ref_pic_list_l1,
        long_term_l0: plan.long_term_l0,
        long_term_l1: plan.long_term_l1,
        num_ref_idx_l0_active: header.num_ref_idx_l0_active,
        num_ref_idx_l1_active: header.num_ref_idx_l1_active,
        wctx: &wctx,
        first_mb_in_slice: header.first_mb_in_slice,
        slice_qp,
        is_i_slice: header.slice_type == SliceType::I,
        cabac_init_idc: header.cabac_init_idc,
    };

    // In MBAFF, first_mb_in_slice is a pair address
    let mut mb_idx = if MBAFF {
        (header.first_mb_in_slice as usize) * 2
    } else {
        header.first_mb_in_slice as usize
    };
    if mb_idx >= total_mbs {
        return Err(DecodeError::InvalidSyntax("first_mb_in_slice out of range"));
    }
    // MBs of this slice from `fin_from` on still need their deblocking info
    // filled in (finalize_mb_info_range).
    let mut fin_from = mb_idx;
    let unit_len = deblock_unit_len(mb_width as usize, MBAFF);
    while mb_idx < total_mbs {
        // Frame threading: deblock and publish rows as they become final.
        if let Some(sync) = sync.as_mut() {
            let ready = sync.ready_mb(mb_idx, unit_len);
            if ready >= (sync.next_unit + 2) * unit_len {
                if ready > fin_from {
                    ctx.finalize_mb_info_range(fin_from, ready, &params);
                    fin_from = ready;
                }
                sync.advance(
                    ready,
                    ctx.frame,
                    ctx.mb_info,
                    mb_width as usize,
                    deblock::Interlace {
                        mbaff_field: MBAFF.then_some(&*ctx.mb_field_decoding),
                        field_pic: header.field_pic_flag,
                    },
                    ctx.mb_slice_id,
                    &slice_deblock,
                    ps_chroma_qp_offset,
                    ps_second_chroma_qp_offset,
                );
            }
        }
        // CAVLC end-of-slice: check before reading any new syntax elements.
        // Skip this check when counting down a skip run (no reads needed).
        if !use_cabac && mb_skip_run <= 0 && !reader.more_rbsp_data() {
            break;
        }

        // Stamp this MB with the current slice ID for boundary detection
        ctx.mb_slice_id[mb_idx] = this_slice_id;
        // MBAFF: a pair's mb_field_decoding_flag defaults to the inferred
        // value (spec 7.4.4: left pair in the same slice, else the pair
        // above, else frame). It is used as is when neither MB of the pair
        // carries the flag, and for context derivation until it is decoded.
        if MBAFF && mb_idx.is_multiple_of(2) {
            let pair = mb_idx / 2;
            let mbw = mb_width as usize;
            ctx.mb_field_decoding[pair] =
                if !pair.is_multiple_of(mbw) && ctx.mb_slice_id[mb_idx - 2] == this_slice_id {
                    ctx.mb_field_decoding[pair - 1]
                } else if pair >= mbw && ctx.mb_slice_id[mb_idx - 2 * mbw] == this_slice_id {
                    ctx.mb_field_decoding[pair - mbw]
                } else {
                    false
                };
        }
        // Compute pixel position
        let (mb_x, mb_y) = if MBAFF {
            let pair_addr = mb_idx / 2;
            let pair_col = pair_addr % mb_width as usize;
            let pair_row = pair_addr / mb_width as usize;
            let x = pair_col * 16;
            let y = pair_row * 32 + (mb_idx % 2) * 16;
            (x, y)
        } else {
            (
                (mb_idx % mb_width as usize) * 16,
                (mb_idx / mb_width as usize) * 16,
            )
        };

        // CABAC decode path
        if use_cabac {
            let cr = cabac_reader.as_mut().unwrap();
            let st = &mut cabac_state;

            // MBAFF: end_of_slice_flag and mb_field_decoding_flag are both
            // handled inside decode_cabac_mb (contexts 70-72 for field flag,
            // with MBAFF-adjusted first_mb comparison for terminate).
            match ctx.decode_cabac_mb(cr, st, rbsp, mb_idx, mb_x, mb_y, &params)? {
                CabacMbResult::EndOfSlice => break,
                CabacMbResult::Decoded => {}
            }
            if cr.overread() {
                return Err(DecodeError::InvalidSyntax(
                    "CABAC read past the end of the slice data",
                ));
            }
            if MBAFF && mb_idx % 2 != 0 {
                let term = cr.get_cabac_terminate();
                if term != 0 {
                    mb_idx += 1;
                    break;
                }
            }

            mb_idx += 1;
            continue;
        }

        // P/B-slice skip run handling
        if is_p_slice || is_b_slice {
            if mb_skip_run < 0 {
                mb_skip_run = reader.read_ue()? as i32;
            }
            if mb_skip_run > 0 {
                mb_skip_run -= 1;
                ctx.mb_skip[mb_idx] = true; // Mark as skipped for MBAFF field flag inference
                                            // MBAFF: when a run ends on a skipped top MB, the bottom MB is
                                            // coded and its mb_field_decoding_flag (the next bit) applies
                                            // to the whole pair, so read it before decoding this MB.
                if MBAFF && mb_idx.is_multiple_of(2) && mb_skip_run == 0 {
                    ctx.mb_field_decoding[mb_idx / 2] = reader.read_bit()? != 0;
                }
                // Set layout for skip MBs (field_flag already known)
                ctx.set_mb_layout(mb_idx, mb_x, mb_y);
                if is_p_slice {
                    // P_Skip: MV = median predictor, ref_idx = 0, no residual
                    ctx.decode_p_skip_mb(mb_idx, mb_x, mb_y, &params);
                } else {
                    // B_Skip: spatial/temporal direct MV + MC, no residual
                    ctx.decode_b_skip_mb(mb_idx, mb_x, mb_y, &params);
                }
                ctx.mb_info[mb_idx].set_type_qp(MbType::Inter, ctx.prev_mb_qp);
                mb_idx += 1;
                continue;
            }
            // mb_skip_run == 0: parse the next MB normally
            mb_skip_run = -1; // reset for next iteration

            // MBAFF: read mb_field_decoding_flag for this pair
            // (spec 7.3.4: read before first non-skipped MB of pair)
            if MBAFF {
                // A bottom MB after a skipped top MB had its flag read with
                // the top MB (above), so only top MBs read it here.
                if mb_idx.is_multiple_of(2) {
                    ctx.mb_field_decoding[mb_idx / 2] = reader.read_bit()? != 0;
                }
            }
        }

        // MBAFF I-slice: read mb_field_decoding_flag before MB decode
        if MBAFF && !(is_p_slice || is_b_slice) {
            let is_top = mb_idx % 2 == 0;
            if is_top {
                ctx.mb_field_decoding[mb_idx / 2] = reader.read_bit()? != 0;
            }
        }

        ctx.set_mb_layout(mb_idx, mb_x, mb_y);
        ctx.decode_cavlc_mb(&mut reader, mb_idx, mb_x, mb_y, &params)?;
        mb_idx += 1;

        // CAVLC end-of-slice: spec says "while (more_rbsp_data())"
        // after each MB. For single-slice, this naturally ends at
        // total_mbs. For multi-slice, it stops at each slice boundary.
        if !use_cabac && !reader.more_rbsp_data() {
            break;
        }
    }

    // Post-loop: fill deblock info for the MBs not done by the row hook.
    let end_mb = mb_idx.min(total_mbs);
    ctx.finalize_mb_info_range(fin_from, end_mb, &params);
    let l0 = if is_p_slice {
        ref_pic_list
    } else {
        _ref_pic_list_l0
    };
    let keys = ReferenceKeys::new([l0, _ref_pic_list_l1], header.field_pic_flag);
    reference_keys.resize_with(usize::from(this_slice_id), || keys.clone());
    reference_keys.push(keys);
    if let Some(sync) = sync.as_mut() {
        let ready = sync.ready_mb(end_mb, unit_len);
        sync.advance(
            ready,
            ctx.frame,
            ctx.mb_info,
            mb_width as usize,
            deblock::Interlace {
                mbaff_field: MBAFF.then_some(&*ctx.mb_field_decoding),
                field_pic: header.field_pic_flag,
            },
            ctx.mb_slice_id,
            &slice_deblock,
            ps_chroma_qp_offset,
            ps_second_chroma_qp_offset,
        );
    }

    let prev_mb_qp = ctx.prev_mb_qp;
    let last_qp_delta_nonzero = ctx.last_qp_delta_nonzero;

    // Store state back into pending PictureState.
    // Deblocking and DPB insertion happen in finalize_pending().
    Ok(PictureState {
        frame,
        frame_num: header.frame_num,
        poc: current_poc,
        field_poc: ps_field_poc,
        nal_unit_type: plan.nal_unit_type,
        nal_ref_idc: plan.nal_ref_idc,
        scratch: PictureScratch {
            nc_luma,
            nc_cb,
            nc_cr,
            mv_store_l0,
            mv_store_l1,
            ref_idx_store_l0,
            reference_keys,
            ref_idx_store_l1,
            mvd_store,
            mvd_store_l1,
            mb_info,
            i4x4_modes,
            mb_cbp,
            mb_chroma_pred,
            mb_is_8x8dct,
            mb_skip,
            mb_is_direct,
            blk_is_direct,
            is_i16x16,
            mb_slice_id,
            mb_field_decoding: _mb_field_decoding,
            slice_deblock,
        },
        current_slice_id,
        prev_mb_qp,
        last_qp_delta_nonzero,
        mmco_ops: header.mmco_ops.clone(),
        adaptive_ref_pic_marking: header.adaptive_ref_pic_marking,
        long_term_reference_flag: header.long_term_reference_flag,
        is_intra_slice: header.slice_type == SliceType::I,
        chroma_qp_index_offset: ps_chroma_qp_offset,
        second_chroma_qp_index_offset: ps_second_chroma_qp_offset,
        mb_width,
        mb_height,
        mbaff_frame_flag: header.mbaff_frame_flag,
        field_pic_flag: header.field_pic_flag,
        bottom_field_flag: header.bottom_field_flag,
        frame_height,
        max_frame_num,
        crop_x,
        crop_y,
    })
}

fn new_picture_frame<P: Pixel>(plan: &SlicePlan, recycled: Option<Frame>) -> Frame {
    fn zeroed(mut plane: Vec<u8>, len: usize) -> Vec<u8> {
        if plane.capacity() < len {
            return vec![0; len];
        }
        plane.clear();
        plane.resize(len, 0);
        plane
    }

    let chroma_format = ChromaFormat::from_idc(plan.sps.chroma_format_idc);
    let chroma_len = ((plan.coded_width / 2) * (plan.coded_height >> chroma_format.shift_y()))
        as usize
        * P::BYTES;
    let luma_len = (plan.coded_width * plan.coded_height) as usize * P::BYTES;
    let bit_depth = P::BIT_DEPTH as u8;
    let [y, u, v] = recycled
        .filter(|frame| {
            frame.width == plan.width
                && frame.height == plan.height
                && frame.chroma_format == chroma_format
                && frame.bit_depth == bit_depth
        })
        .map_or_else(<[Vec<u8>; 3]>::default, |frame| [frame.y, frame.u, frame.v]);
    Frame {
        width: plan.width,
        height: plan.height,
        y: zeroed(y, luma_len),
        u: zeroed(u, chroma_len),
        v: zeroed(v, chroma_len),
        pic_order_cnt: plan.current_poc,
        chroma_format,
        bit_depth,
    }
}

/// Apply the in-loop deblocking filter to a fully decoded picture.
pub(crate) fn deblock_picture(ps: &mut PictureState) {
    deblock::filter_frame_mbaff(
        &mut ps.frame,
        &ps.scratch.mb_info,
        ps.mb_width as usize,
        &ps.scratch.mb_slice_id,
        &ps.scratch.slice_deblock,
        ps.chroma_qp_index_offset,
        ps.second_chroma_qp_index_offset,
        deblock::Interlace {
            mbaff_field: ps
                .mbaff_frame_flag
                .then_some(&ps.scratch.mb_field_decoding[..]),
            field_pic: ps.field_pic_flag,
        },
    );
}

/// Metadata of the DPB picture for a picture being decoded into `ps`.
pub(crate) fn picture_meta(ps: &PictureState) -> PictureMeta {
    let (frame_num, pic_order_cnt, field_poc) =
        decoded_numbering(&ps.mmco_ops, ps.frame_num, ps.poc, ps.field_poc);
    PictureMeta {
        width: ps.mb_width * 16,
        // Coded height of this picture's planes, not the display height rounded
        // up: cropping may remove a macroblock or more.
        height: ps.mb_height * 16,
        frame_num,
        pic_order_cnt,
        field_poc,
        mb_width: ps.mb_width,
        is_intra: ps.is_intra_slice,
        structure: if ps.field_pic_flag {
            if ps.bottom_field_flag {
                crate::dpb::PictureStructure::BottomField
            } else {
                crate::dpb::PictureStructure::TopField
            }
        } else {
            crate::dpb::PictureStructure::Frame
        },
        chroma_422: ps.frame.chroma_format == ChromaFormat::Yuv422,
        sample_bytes: if ps.frame.bit_depth > 8 { 2 } else { 1 },
    }
}

/// Move the motion data out of a fully decoded picture.
pub(crate) fn take_motion(ps: &mut PictureState) -> Motion {
    Motion {
        mv_l0: core::mem::take(&mut ps.scratch.mv_store_l0),
        ref_idx_l0: core::mem::take(&mut ps.scratch.ref_idx_store_l0),
        slice_ids: core::mem::take(&mut ps.scratch.mb_slice_id),
        reference_keys: core::mem::take(&mut ps.scratch.reference_keys),
        mv_l1: core::mem::take(&mut ps.scratch.mv_store_l1),
        ref_idx_l1: core::mem::take(&mut ps.scratch.ref_idx_store_l1),
        mb_field: if ps.mbaff_frame_flag {
            ps.scratch.mb_field_decoding.clone()
        } else {
            Vec::new()
        },
    }
}

/// Copy the coded planes into the DPB and move the picture's motion data.
fn make_decoded_picture(
    ps: &mut PictureState,
    spare: Option<Arc<DecodedPicture>>,
) -> Arc<DecodedPicture> {
    let meta = picture_meta(ps);
    let motion = take_motion(ps);
    let [y, u, v] = [&ps.frame.y, &ps.frame.u, &ps.frame.v];
    let motion = if let Some(mut spare) = spare {
        let picture = Arc::get_mut(&mut spare).expect("retired picture is uniquely owned");
        match picture.reuse_complete(meta, [y, u, v], motion) {
            Ok(()) => return spare,
            Err(motion) => motion,
        }
    } else {
        motion
    };
    Arc::new(DecodedPicture::new_complete(
        meta,
        y.clone(),
        u.clone(),
        v.clone(),
        motion,
    ))
}

/// Number of macroblocks in one deblocking unit: an MB row, or an MB-pair
/// row with MBAFF (pairs are decoded and deblocked together).
#[inline]
pub(crate) fn deblock_unit_len(mb_width: usize, mbaff: bool) -> usize {
    if mbaff {
        2 * mb_width
    } else {
        mb_width
    }
}

/// Incremental deblocking and row publication for a picture decoded by a
/// frame-threading worker, so that pictures referencing it can start before
/// it is finished (see `threading`).
///
/// Intra prediction reads *unfiltered* neighbouring samples from the unit
/// above, so unit `u` is deblocked only once unit `u + 1` is decoded too (and
/// no later slice of the picture can write to either). Deblocking a unit
/// also modifies up to three lines at the bottom of the unit above, so after
/// deblocking units `0..n` the rows of units `0..n-1` are final and get
/// published.
pub(crate) struct RowSync {
    pub(crate) writer: PictureWriter,
    /// Next deblocking unit of the private frame.
    pub(crate) next_unit: usize,
    /// MB index from which a later slice of this picture may still write.
    pub(crate) limit_mb: usize,
}

impl RowSync {
    /// MB index up to which whole units are ready to deblock once MBs
    /// `..done_mb` are decoded.
    #[inline]
    fn ready_mb(&self, done_mb: usize, unit_len: usize) -> usize {
        if unit_len == 0 {
            return 0;
        }
        done_mb.min(self.limit_mb) / unit_len * unit_len
    }

    /// Deblock every unit that ends at or before `ready_mb`, then publish
    /// the rows that are final.
    #[allow(clippy::too_many_arguments)]
    fn advance(
        &mut self,
        ready_mb: usize,
        frame: &mut Frame,
        mb_info: &[MbInfo],
        mb_width: usize,
        interlace: deblock::Interlace,
        mb_slice_id: &[u16],
        slice_deblock: &[deblock::SliceDeblock],
        chroma_qp_offset: i32,
        second_chroma_qp_offset: i32,
    ) {
        let mbaff = interlace.mbaff_field.is_some();
        let unit_len = deblock_unit_len(mb_width, mbaff);
        if unit_len == 0 {
            return;
        }
        // Units below `target` have their successor decoded as well.
        let target = (ready_mb / unit_len).saturating_sub(1);
        if target <= self.next_unit {
            return;
        }
        while self.next_unit < target {
            let start = self.next_unit * unit_len;
            deblock::filter_mb_range(
                frame,
                mb_info,
                mb_width,
                mb_slice_id,
                slice_deblock,
                chroma_qp_offset,
                second_chroma_qp_offset,
                interlace,
                start..start + unit_len,
            );
            self.next_unit += 1;
        }
        let rows_per_unit = if mbaff { 2 } else { 1 };
        let final_rows = (self.next_unit - 1) * rows_per_unit;
        self.writer
            .publish(&frame.y, &frame.u, &frame.v, final_rows as u32);
    }

    /// Deblock the rest of a fully decoded picture and complete the shared
    /// picture.
    #[cfg_attr(not(feature = "std"), allow(dead_code))]
    pub(crate) fn finish(mut self, ps: &mut PictureState) {
        let unit_len = deblock_unit_len(ps.mb_width as usize, ps.mbaff_frame_flag);
        let start = self.next_unit * unit_len;
        {
            deblock::filter_mb_range(
                &mut ps.frame,
                &ps.scratch.mb_info,
                ps.mb_width as usize,
                &ps.scratch.mb_slice_id,
                &ps.scratch.slice_deblock,
                ps.chroma_qp_index_offset,
                ps.second_chroma_qp_index_offset,
                deblock::Interlace {
                    mbaff_field: ps
                        .mbaff_frame_flag
                        .then_some(&ps.scratch.mb_field_decoding[..]),
                    field_pic: ps.field_pic_flag,
                },
                start..ps.scratch.mb_info.len(),
            );
        }
        self.next_unit = ps.scratch.mb_info.len().div_ceil(unit_len.max(1));
        let motion = take_motion(ps);
        self.writer
            .finish(&ps.frame.y, &ps.frame.u, &ps.frame.v, motion);
    }
}

/// Decoded reference picture marking (spec 8.2.5) for a picture that has
/// just been decoded: IDR handling, MMCO operations, insertion with sliding
/// window marking, and MMCO 6 after insertion. `pic` only needs correct
/// metadata (frame_num, POC, structure) for the DPB bookkeeping itself.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_reference_marking(
    dpb: &mut Dpb,
    pic: Arc<DecodedPicture>,
    nal_unit_type: NalUnitType,
    nal_ref_idc: u8,
    frame_num: u32,
    max_frame_num: u32,
    field_pic_flag: bool,
    mmco_ops: &[(u32, u32)],
    adaptive_ref_pic_marking: bool,
    long_term_reference_flag: bool,
) {
    if nal_unit_type == NalUnitType::SliceIdr {
        dpb.clear();
    }

    let reference = if nal_ref_idc > 0 {
        if nal_unit_type == NalUnitType::SliceIdr && long_term_reference_flag {
            ReferenceStatus::LongTerm(0) // IDR with long_term_reference_flag → LT idx 0
        } else {
            ReferenceStatus::ShortTerm
        }
    } else {
        ReferenceStatus::Unused
    };

    // For MMCO, use CurrPicNum (= 2*frame_num+1 for field pictures)
    let mmco_curr_pic_num = if field_pic_flag {
        (frame_num * 2 + 1) as i32
    } else {
        frame_num as i32
    };
    // A command names a picture by PicNum, counted back from CurrPicNum
    // (spec 8.2.5.4.1), negative for a frame once frame_num has wrapped. In
    // a field picture PicNum is 2 * FrameNumWrap + 1 for a field of the
    // current parity and 2 * FrameNumWrap for the other (spec 8.2.4.1);
    // LongTermPicNum likewise with LongTermFrameIdx.
    let current_parity = pic.structure;
    let field_of = |num: i32| -> Option<PictureStructure> {
        field_pic_flag.then(|| {
            if num & 1 == 1 {
                current_parity
            } else if current_parity == PictureStructure::TopField {
                PictureStructure::BottomField
            } else {
                PictureStructure::TopField
            }
        })
    };
    let frame_num_of = |pic_num: i32| -> u32 {
        let wrap = if field_pic_flag {
            pic_num.div_euclid(2)
        } else {
            pic_num
        };
        Dpb::frame_num_for_pic_num(wrap, max_frame_num)
    };
    for &(op, param) in mmco_ops {
        match op {
            1 => {
                let pic_num_to_remove = mmco_curr_pic_num - (param as i32 + 1);
                dpb.mark_short_term_unused(
                    frame_num_of(pic_num_to_remove),
                    field_of(pic_num_to_remove),
                );
            }
            2 => {
                let idx = if field_pic_flag { param / 2 } else { param };
                dpb.mark_long_term_unused(idx, field_of(param as i32));
            }
            3 => {
                let abs_diff_minus1 = param & ((1 << MMCO_DIFF_BITS) - 1);
                let long_term_frame_idx = param >> MMCO_DIFF_BITS;
                let pic_num = mmco_curr_pic_num - (abs_diff_minus1 as i32 + 1);
                dpb.assign_long_term(
                    frame_num_of(pic_num),
                    field_of(pic_num),
                    long_term_frame_idx,
                );
            }
            4 => {
                dpb.set_max_long_term_frame_idx(param);
            }
            5 => {
                // After op=5 the current picture is treated as frame_num 0;
                // the encoder guarantees that, we only need the DPB cleared.
                dpb.clear_all_refs();
            }
            6 => {
                // Applied after insert (current pic must be in DPB first)
            }
            _ => {}
        }
    }

    // The sliding window applies only without adaptive marking (spec
    // 8.2.5.3); an IDR clears the DPB instead
    let sliding_window = !adaptive_ref_pic_marking && nal_unit_type != NalUnitType::SliceIdr;
    dpb.insert_marked(pic, reference, sliding_window);
    if nal_ref_idc > 0 {
        // A picture with MMCO 5 continues as frame_num 0
        dpb.set_prev_ref_frame_num(if has_mmco5(mmco_ops) { 0 } else { frame_num });
    }

    for &(op, param) in mmco_ops {
        if op == 6 {
            // A picture with MMCO 5 continues as frame_num 0
            let frame_num = if has_mmco5(mmco_ops) { 0 } else { frame_num };
            dpb.mark_current_as_long_term(param, frame_num);
        }
    }
}

/// Turn a finalized picture into an output frame: crop the coded (MB-aligned)
/// planes to the display size and, for field pictures, interleave the two
/// fields of a frame (`pending_field` holds the first field until its
/// complement arrives). Returns `None` when nothing is ready for output.
/// Whether `mmco_ops` holds memory_management_control_operation 5.
pub(crate) fn has_mmco5(mmco_ops: &[(u32, u32)]) -> bool {
    mmco_ops.iter().any(|&(op, _)| op == 5)
}

/// frame_num, POC and field POCs a picture keeps after decoding: a picture
/// with MMCO 5 continues as frame_num 0 with its POCs moved so that the
/// smaller one is 0 (spec 8.2.1, 8.2.5.4.5).
pub(crate) fn decoded_numbering(
    mmco_ops: &[(u32, u32)],
    frame_num: u32,
    poc: i32,
    field_poc: [i32; 2],
) -> (u32, i32, [i32; 2]) {
    if has_mmco5(mmco_ops) {
        (0, 0, field_poc.map(|p| p.wrapping_sub(poc)))
    } else {
        (frame_num, poc, field_poc)
    }
}

pub(crate) fn finish_output(
    mut ps: PictureState,
    pending_field: &mut Option<Frame>,
) -> Option<Frame> {
    if has_mmco5(&ps.mmco_ops) {
        // Output follows the reset POC: first of a new output period
        ps.frame.pic_order_cnt = 0;
    }

    crop_display(
        &mut ps.frame,
        (ps.mb_width * 16) as usize,
        (ps.crop_x, ps.crop_y),
    )?;

    // For field pictures, combine two fields into one frame for output
    if ps.field_pic_flag {
        let field_frame = ps.frame;
        if let Some(first_field) = pending_field.take() {
            // Second field arrived — combine into a full frame. Rows are
            // copied as bytes: `w` and `cw` are row lengths in bytes.
            let width = field_frame.width as usize;
            let bit_depth = field_frame.bit_depth;
            let w = width * field_frame.bytes_per_sample();
            let full_h = ps.frame_height as usize;
            let field_h = full_h / 2;
            let mut y = vec![0u8; w * full_h];
            let chroma_format = field_frame.chroma_format;
            let sy = chroma_format.shift_y();
            let mut u = vec![0u8; (w / 2) * (full_h >> sy)];
            let mut v = vec![0u8; (w / 2) * (full_h >> sy)];
            let cw = w / 2;

            // The two fields must agree on geometry and hold enough samples;
            // a corrupt stream can change SPS dimensions between fields.
            let ch = field_h >> sy;
            let field_ok = |f: &Frame| {
                f.width as usize == width
                    && f.bit_depth == bit_depth
                    && f.y.len() >= field_h * w
                    && f.u.len() >= ch * cw
                    && f.v.len() >= ch * cw
            };
            if w == 0 || full_h < 2 || !field_ok(&first_field) || !field_ok(&field_frame) {
                // Drop the stale field and hold the current one instead.
                *pending_field = Some(field_frame);
                return None;
            }

            // Determine which is top and which is bottom
            let (top, bot) = if ps.bottom_field_flag {
                (&first_field, &field_frame)
            } else {
                (&field_frame, &first_field)
            };

            // Interleave luma lines: top→even, bottom→odd
            for r in 0..field_h {
                let src_off = r * w;
                y[r * 2 * w..r * 2 * w + w].copy_from_slice(&top.y[src_off..src_off + w]);
                y[(r * 2 + 1) * w..(r * 2 + 1) * w + w]
                    .copy_from_slice(&bot.y[src_off..src_off + w]);
            }
            // Interleave chroma lines
            for r in 0..ch {
                let src_off = r * cw;
                u[r * 2 * cw..r * 2 * cw + cw].copy_from_slice(&top.u[src_off..src_off + cw]);
                u[(r * 2 + 1) * cw..(r * 2 + 1) * cw + cw]
                    .copy_from_slice(&bot.u[src_off..src_off + cw]);
                v[r * 2 * cw..r * 2 * cw + cw].copy_from_slice(&top.v[src_off..src_off + cw]);
                v[(r * 2 + 1) * cw..(r * 2 + 1) * cw + cw]
                    .copy_from_slice(&bot.v[src_off..src_off + cw]);
            }

            return Some(Frame {
                y,
                u,
                v,
                width: width as u32,
                height: full_h as u32,
                pic_order_cnt: field_frame.pic_order_cnt.min(first_field.pic_order_cnt),
                chroma_format,
                bit_depth,
            });
        } else {
            // First field — hold it, return None
            *pending_field = Some(field_frame);
            return None;
        }
    }

    Some(ps.frame)
}

fn crop_display(frame: &mut Frame, coded_w: usize, origin: (u32, u32)) -> Option<()> {
    // Crop the coded (MB-aligned) planes to the display window, which starts
    // at (crop_x, crop_y) (spec 7.4.2.1.1, frame_crop_*_offset).
    if coded_w == 0 {
        return None;
    }
    let bps = frame.bytes_per_sample();
    let coded_h = frame.y.len() / (coded_w * bps);
    let display_w = frame.width as usize;
    let display_h = frame.height as usize;
    let (x0, y0) = (origin.0 as usize, origin.1 as usize);
    if coded_w != display_w || coded_h != display_h || x0 != 0 || y0 != 0 {
        // The window must lie inside the coded picture and the frame buffer
        if x0 + display_w > coded_w
            || y0 + display_h > coded_h
            || coded_w * coded_h * bps > frame.y.len()
        {
            return None;
        }
        let sy = frame.chroma_format.shift_y();
        let chroma_coded_w = coded_w / 2;
        let chroma_w = display_w / 2;
        let chroma_h = display_h >> sy;
        let (cx0, cy0) = (x0 / 2, y0 >> sy);

        if x0 == 0 && y0 == 0 && coded_w == display_w {
            // Bottom crop only: the display rows are a prefix of the planes
            frame.y.truncate(display_w * display_h * bps);
            frame.u.truncate(chroma_w * chroma_h * bps);
            frame.v.truncate(chroma_w * chroma_h * bps);
        } else {
            // In bytes: stride, x and w scaled by the sample size
            let crop =
                |plane: &mut Vec<u8>, stride: usize, x: usize, y: usize, w: usize, h: usize| {
                    let (stride, x, w) = (stride * bps, x * bps, w * bps);
                    for r in 0..h {
                        let src = (y + r) * stride + x;
                        plane.copy_within(src..src + w, r * w);
                    }
                    plane.truncate(w * h);
                };
            crop(&mut frame.y, coded_w, x0, y0, display_w, display_h);
            crop(&mut frame.u, chroma_coded_w, cx0, cy0, chroma_w, chroma_h);
            crop(&mut frame.v, chroma_coded_w, cx0, cy0, chroma_w, chroma_h);
        }
    }

    Some(())
}

/// H.264 decoder with built-in display-order reordering.
///
/// Wraps [`Decoder`] with a reorder buffer that emits frames in display order
/// (sorted by picture order count) instead of decode order. This eliminates
/// the need for callers to track GOP boundaries and sort frames manually,
/// and avoids a common pitfall around IDR boundary handling.
///
/// Use this when you want to display, render, or write frames in their
/// intended visual order. Use [`Decoder`] directly if you need raw decode
/// order or want to manage reordering yourself.
///
/// # How it works
///
/// Internally tracks GOP boundaries (each IDR starts a new GOP) and buffers
/// decoded frames. Frames are emitted in `(gop_id, pic_order_cnt)` order.
/// On each IDR boundary, all buffered frames from the previous GOP are
/// drained and returned. The buffer also has a maximum depth (16 frames)
/// to bound latency for streams with infrequent IDRs.
///
/// `OrderedDecoder<M>` can also carry per-picture metadata through the same
/// reorder buffer with [`decode_nal_with_meta`](Self::decode_nal_with_meta) and
/// [`flush_with_meta`](Self::flush_with_meta). This is useful for timestamps
/// such as PTS values: metadata is attached to the picture being built and is
/// moved to the frame when the inner decoder finalizes that picture one NAL
/// later, so B-frame reordering does not break frame/timestamp association.
/// The default metadata type is `()`, so existing `decode_nal` and `flush`
/// callers are unchanged.
///
/// # Example
///
/// ```no_run
/// use deadlib_video_h264::decoder::OrderedDecoder;
/// use deadlib_video_h264::nal::parse_annex_b;
///
/// let bitstream = std::fs::read("input.h264").unwrap();
/// let nals = parse_annex_b(&bitstream);
/// let mut decoder = OrderedDecoder::new();
///
/// for nal in &nals {
///     // decode_nal returns 0 or more frames in display order
///     for frame in decoder.decode_nal(nal).unwrap() {
///         println!("Display POC={}", frame.pic_order_cnt);
///     }
/// }
/// // Drain any remaining buffered frames
/// for frame in decoder.flush() {
///     println!("Final POC={}", frame.pic_order_cnt);
/// }
/// ```
pub struct OrderedDecoder<M = ()> {
    inner: Decoder,
    /// Buffered frames awaiting display-order release. Each entry is
    /// `(gop_id, frame, meta)` so frames from different GOPs don't interleave.
    buffer: Vec<(u32, Frame, Option<M>)>,
    /// Metadata for the picture currently being assembled by the inner decoder.
    /// When `Decoder::decode_nal` finalizes that picture one NAL later, this
    /// value is moved into the reorder buffer with the returned frame.
    building_meta: Option<M>,
    /// Monotonically incrementing GOP id, bumped on each IDR boundary.
    gop_id: u32,
    /// Maximum frames to keep in the buffer before forcing the lowest
    /// (oldest) one out. Bounds reorder latency for streams with
    /// infrequent IDRs.
    max_depth: usize,
}

impl<M> Default for OrderedDecoder<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> OrderedDecoder<M> {
    /// Return a consumed output frame for reuse by the decoder.
    pub fn recycle_frame(&mut self, frame: Frame) {
        self.inner.recycle_frame(frame);
    }

    /// Create a new ordering decoder with a default reorder buffer depth of
    /// 16 frames (sufficient for any practical bframes setting). Existing
    /// callers receive frames only; use [`decode_nal_with_meta`](OrderedDecoder::decode_nal_with_meta)
    /// on `OrderedDecoder<M>` to carry per-picture metadata through reordering.
    pub fn new() -> Self {
        Self::with_max_depth(16)
    }

    /// Create a new ordering decoder with a custom maximum buffer depth.
    /// Larger values give more reordering headroom for unusual bitstreams
    /// but increase end-to-end latency. Existing callers receive frames only;
    /// use [`decode_nal_with_meta`](OrderedDecoder::decode_nal_with_meta) on
    /// `OrderedDecoder<M>` to carry per-picture metadata through reordering.
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            inner: Decoder::new(),
            buffer: Vec::new(),
            building_meta: None,
            gop_id: 0,
            max_depth: max_depth.max(1),
        }
    }
}

impl OrderedDecoder<()> {
    /// Feed a single NAL unit and return any frames that are now ready
    /// for display, in display order.
    ///
    /// Most NALs return an empty `Vec` (parameter sets, mid-GOP slices that
    /// don't yet free up the head of the buffer). When the internal buffer
    /// fills up or when an IDR boundary completes a GOP, one or more frames
    /// are released.
    pub fn decode_nal(&mut self, nal: &NalUnit) -> Result<Vec<Frame>, DecodeError> {
        Ok(self
            .decode_nal_with_meta(nal, ())?
            .into_iter()
            .map(|(frame, ())| frame)
            .collect())
    }

    /// Flush all remaining buffered frames at end-of-stream, in display order.
    /// Call this once after the last `decode_nal`.
    pub fn flush(&mut self) -> Vec<Frame> {
        self.flush_with_meta()
            .into_iter()
            .map(|(frame, ())| frame)
            .collect()
    }
}

impl<M> OrderedDecoder<M> {
    /// Feed a single NAL unit with per-picture metadata and return any frames
    /// that are now ready for display, in display order.
    ///
    /// # Arguments
    ///
    /// * `nal` - The NAL unit to decode.
    /// * `meta` - Metadata for the access unit/picture that starts at this NAL.
    ///   Metadata is consumed only for slice NALs (`Slice` or `SliceIdr`) that
    ///   start or bootstrap a picture. Metadata passed for SPS, PPS, SEI, and
    ///   other non-slice NALs is ignored. For continuation slices of the same
    ///   picture, the metadata already attached to the first slice is kept.
    ///
    /// # Returns
    ///
    /// A vector of `(Frame, M)` pairs released from the reorder buffer in
    /// display order. Most calls return an empty vector until the reorder buffer
    /// has enough frames or an IDR boundary completes a GOP.
    ///
    /// # Pipeline delay and metadata association
    ///
    /// [`Decoder::decode_nal`] has a one-picture delay: when a slice NAL starts
    /// a new picture, the inner decoder finalizes and returns the previous
    /// picture before decoding the new slice. `OrderedDecoder` therefore stores
    /// the metadata for the picture currently being built and moves it onto the
    /// frame only when that delayed finalization happens. This keeps timestamps
    /// such as PTS values associated with the correct frame through B-frame
    /// reordering.
    ///
    /// A returned frame without stored metadata is treated as an internal logic
    /// error and panics instead of silently corrupting PTS/frame association; for
    /// well-formed slice input, every finalized frame must have been bootstrapped
    /// from a prior slice NAL.
    pub fn decode_nal_with_meta(
        &mut self,
        nal: &NalUnit,
        meta: M,
    ) -> Result<Vec<(Frame, M)>, DecodeError> {
        Ok(self
            .decode_nal_buffered(nal, Some(meta))?
            .into_iter()
            .map(Self::expect_meta)
            .collect())
    }

    /// Flush all remaining buffered frames and metadata at end-of-stream, in
    /// display order.
    ///
    /// # Returns
    ///
    /// A vector of `(Frame, M)` pairs for all frames still held by the inner
    /// decoder or reorder buffer. The final pending picture is paired with the
    /// metadata stored when that picture's first slice was decoded.
    ///
    /// # Pipeline delay and metadata association
    ///
    /// The inner [`Decoder::flush`] returns the last picture that never saw a
    /// following new-picture trigger. `flush_with_meta` pairs that frame with
    /// the currently stored `building_meta`, then drains the reorder buffer by
    /// `(gop_id, pic_order_cnt)` just like [`flush`](OrderedDecoder::flush).
    /// Missing metadata is treated as an internal logic error and panics rather
    /// than returning a frame with the wrong caller metadata.
    pub fn flush_with_meta(&mut self) -> Vec<(Frame, M)> {
        self.flush_buffered()
            .into_iter()
            .map(Self::expect_meta)
            .collect()
    }

    fn decode_nal_buffered(
        &mut self,
        nal: &NalUnit,
        meta: Option<M>,
    ) -> Result<Vec<(Frame, Option<M>)>, DecodeError> {
        let mut output = Vec::new();
        let is_idr = nal.nal_unit_type == NalUnitType::SliceIdr;
        let is_slice = matches!(
            nal.nal_unit_type,
            NalUnitType::Slice | NalUnitType::SliceIdr
        );

        // Decode the NAL. Note that decode_nal returns the PREVIOUS frame
        // (the picture that just got finalized when this NAL started a new
        // one), so the returned frame still belongs to the current gop_id.
        let mut finalized_previous = false;
        if let Some(frame) = self.inner.decode_nal(nal)? {
            finalized_previous = true;
            let frame_meta = self.building_meta.take();
            // A picture with MMCO 5 starts a new output period: everything
            // decoded before it is output first (spec C.4.4)
            if self.inner.take_period_start() {
                self.gop_id += 1;
                self.drain_completed_gops(&mut output);
            }
            self.buffer.push((self.gop_id, frame, frame_meta));
        }

        if is_slice && (finalized_previous || self.building_meta.is_none()) {
            self.building_meta = meta;
        }

        // After pushing, advance the GOP if this NAL was an IDR start.
        // This must happen AFTER the push so the previous GOP's last frame
        // gets the correct (old) gop_id.
        if is_idr {
            self.gop_id += 1;
            // The previous GOP is now complete — drain everything from it
            // in display order.
            self.drain_completed_gops(&mut output);
        }

        // Bound buffer depth to limit reorder latency.
        while self.buffer.len() > self.max_depth {
            output.push(self.pop_lowest());
        }

        Ok(output)
    }

    fn flush_buffered(&mut self) -> Vec<(Frame, Option<M>)> {
        // First, get any final pending frame from the inner decoder.
        if let Some(frame) = self.inner.flush() {
            let frame_meta = self.building_meta.take();
            if self.inner.take_period_start() {
                self.gop_id += 1;
            }
            self.buffer.push((self.gop_id, frame, frame_meta));
        }
        // Drain everything in (gop, poc) order.
        self.buffer.sort_by_key(|(g, f, _)| (*g, f.pic_order_cnt));
        self.buffer.drain(..).map(|(_, f, m)| (f, m)).collect()
    }

    /// Frame rate from the most recently parsed SPS's VUI timing info,
    /// as `(numerator, denominator)`. See [`Decoder::frame_rate`] for details.
    pub fn frame_rate(&self) -> Option<(u32, u32)> {
        self.inner.frame_rate()
    }

    /// Frame rate as a single floating-point value.
    /// See [`Decoder::frame_rate_f64`].
    pub fn frame_rate_f64(&self) -> Option<f64> {
        self.inner.frame_rate_f64()
    }

    /// Drain all frames whose `gop_id < self.gop_id`, sorted by display order,
    /// into `output`.
    fn drain_completed_gops(&mut self, output: &mut Vec<(Frame, Option<M>)>) {
        let cur = self.gop_id;
        // Stable partition: keep current-GOP frames, extract older ones.
        let mut completed: Vec<(u32, Frame, Option<M>)> = Vec::new();
        let mut remaining: Vec<(u32, Frame, Option<M>)> = Vec::with_capacity(self.buffer.len());
        for entry in self.buffer.drain(..) {
            if entry.0 < cur {
                completed.push(entry);
            } else {
                remaining.push(entry);
            }
        }
        completed.sort_by_key(|(g, f, _)| (*g, f.pic_order_cnt));
        output.extend(completed.into_iter().map(|(_, f, m)| (f, m)));
        self.buffer = remaining;
    }

    /// Remove and return the buffered frame with the lowest `(gop, poc)`.
    fn pop_lowest(&mut self) -> (Frame, Option<M>) {
        let idx = self
            .buffer
            .iter()
            .enumerate()
            .min_by_key(|(_, (g, f, _))| (*g, f.pic_order_cnt))
            .map(|(i, _)| i)
            .unwrap();
        let (_, frame, meta) = self.buffer.remove(idx);
        (frame, meta)
    }

    fn expect_meta((frame, meta): (Frame, Option<M>)) -> (Frame, M) {
        let meta = meta.expect(
            "OrderedDecoder finalized a frame before metadata was attached to its first slice",
        );
        (frame, meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nal::parse_annex_b;

    /// A 16x16 Baseline stream with a PPS that sets
    /// redundant_pic_cnt_present_flag: an I_PCM IDR picture filled with
    /// `primary`, then a redundant copy filled with `redundant`.
    fn redundant_picture_stream(primary: u8, redundant: u8) -> Vec<u8> {
        use crate::bitstream::BitWriter;
        let mut stream = Vec::new();
        let mut nal = |header: u8, rbsp: Vec<u8>| {
            stream.extend_from_slice(&[0, 0, 0, 1, header]);
            let mut zeros = 0;
            for byte in rbsp {
                if zeros >= 2 && byte <= 3 {
                    stream.push(3);
                    zeros = 0;
                }
                zeros = if byte == 0 { zeros + 1 } else { 0 };
                stream.push(byte);
            }
        };
        nal(0x67, crate::sps::tests::baseline_sps(0, 0, 2, 1));
        let mut pps = BitWriter::default();
        pps.ue(0)
            .ue(0)
            .u(1, 0)
            .u(1, 0)
            .ue(0)
            .ue(0)
            .ue(0)
            .u(1, 0)
            .u(2, 0);
        pps.se(0).se(0).se(0).u(1, 1).u(1, 0).u(1, 1);
        nal(0x68, pps.finish());
        for (redundant_pic_cnt, sample) in [(0, primary), (1, redundant)] {
            let mut w = BitWriter::default();
            w.ue(0).ue(7).ue(0).u(4, 0).ue(0).ue(redundant_pic_cnt);
            w.u(1, 0).u(1, 0).se(0).ue(1);
            w.ue(25).align();
            for _ in 0..384 {
                w.u(8, sample as u32);
            }
            nal(0x65, w.finish());
        }
        stream
    }

    /// Slices of a redundant coded picture (redundant_pic_cnt > 0) are
    /// skipped (spec 7.4.3, as FFmpeg does), so they neither end the primary
    /// picture nor decode as a picture of their own; redundant_pic_cnt is
    /// parsed, so the header fields after it are read from the right place.
    #[test]
    fn test_redundant_pictures_are_skipped() {
        let stream = redundant_picture_stream(0x80, 0x40);
        let mut decoder = Decoder::new();
        let mut frames = Vec::new();
        for nal in parse_annex_b(&stream) {
            frames.extend(decoder.decode_nal(&nal).unwrap());
        }
        frames.extend(decoder.flush());
        assert_eq!(frames.len(), 1);
        assert!(frames[0].y.iter().all(|&v| v == 0x80));

        #[cfg(feature = "std")]
        {
            let mut threaded = crate::threading::ThreadedDecoder::new(2);
            let mut frames = Vec::new();
            for nal in parse_annex_b(&stream) {
                frames.extend(threaded.decode_nal(&nal).unwrap());
            }
            frames.extend(threaded.flush());
            assert_eq!(frames.len(), 1);
            assert!(frames[0].y.iter().all(|&v| v == 0x80));
        }
    }

    #[cfg(feature = "surface")]
    #[test]
    fn retained_surfaces_keep_their_pixels_after_decoder_drop() {
        let mut frames = Vec::new();
        {
            let mut decoder = crate::SurfaceDecoder::with_max_depth(0);
            for sample in [0x80, 0x20] {
                let stream = redundant_picture_stream(sample, 0x40);
                for nal in parse_annex_b(&stream) {
                    decoder
                        .decode_nal(&nal, |surface| frames.push(surface))
                        .unwrap();
                }
            }
            decoder.flush(|surface| frames.push(surface));
        }
        assert_eq!(frames.len(), 2);
        for (surface, sample) in frames.iter().zip([0x80, 0x20]) {
            assert_eq!(surface.size(), (16, 16));
            assert_eq!(surface.bit_depth(), 8);
            assert_eq!(surface.chroma_format(), ChromaFormat::Yuv420);
            for (index, side) in [16, 8, 8].into_iter().enumerate() {
                let plane = surface.plane(index).unwrap();
                let rows = plane.rows().collect::<Vec<_>>();
                assert_eq!(rows.len(), side);
                assert!(rows
                    .iter()
                    .all(|row| row.len() == side && row.iter().all(|&v| v == sample)));
            }
        }
    }

    #[test]
    fn returned_frame_storage_reuses_the_coded_planes() {
        let stream = redundant_picture_stream(0x80, 0x40);
        let nals = parse_annex_b(&stream);
        let mut decoder = Decoder::new();
        for nal in &nals {
            decoder.decode_nal(nal).unwrap();
        }
        let mut first = decoder.flush().unwrap();
        let pointer = first.y.as_ptr();
        first.y.fill(0xff);
        decoder.recycle_frame(first);
        for nal in &nals {
            decoder.decode_nal(nal).unwrap();
        }
        let next = decoder.flush().unwrap();
        assert_eq!(next.y.as_ptr(), pointer);
        assert!(next.y.iter().all(|&sample| sample == 0x80));
    }
}
