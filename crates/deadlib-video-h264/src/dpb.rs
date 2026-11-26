//! Decoded Picture Buffer (H.264 spec 8.2.5).
//!
//! Stores decoded reference frames for use by P/B slice motion compensation.
//! Manages short-term reference marking via sliding window (spec 8.2.5.3).

use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::nal::NalUnitType;
use crate::once::OnceLock;
use crate::picture_sync::{RowProgress, SharedPlane};
use crate::slice::SliceHeader;
use crate::sps::Sps;

/// Reference status of a picture in the DPB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceStatus {
    /// Not used for reference.
    Unused,
    /// Short-term reference (identified by frame_num).
    ShortTerm,
    /// Long-term reference (identified by long_term_frame_idx).
    LongTerm(u32),
}

/// Whether a picture is a frame or a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureStructure {
    Frame,
    TopField,
    BottomField,
}

/// Motion data of a decoded picture, read as the co-located picture by
/// direct-mode prediction.
#[derive(Debug, Default)]
pub struct Motion {
    /// Per-4x4-block L0 motion vectors (for temporal direct mode co-located access).
    pub mv_l0: Vec<[i16; 2]>,
    /// Per-4x4-block L0 reference indices.
    pub ref_idx_l0: Vec<i8>,
    /// Per-4x4-block POC of the L0 reference picture used (for temporal direct mode mapping).
    pub ref_poc_l0: Vec<i32>,
    /// Per-4x4-block POC of the L1 reference frame used (temporal direct
    /// mode maps it into the current L0 list when L0 is unused).
    pub ref_poc_l1: Vec<i32>,
    /// Per-4x4-block L1 motion vectors (for spatial direct co-located L1 check).
    pub mv_l1: Vec<[i16; 2]>,
    /// Per-4x4-block L1 reference indices (for spatial direct co-located L1 check).
    pub ref_idx_l1: Vec<i8>,
    /// MBAFF pictures: `mb_field_decoding_flag` per MB pair (empty otherwise).
    /// Direct mode maps a co-located pair coded differently from the current
    /// MB (frame vs field) per spec Table 8-6.
    pub mb_field: Vec<bool>,
}

impl Motion {
    /// Motion data for a picture whose content is unknown (all blocks intra).
    fn unavailable(total_mbs: usize) -> Self {
        let n = total_mbs * 16;
        Self {
            mv_l0: vec![[0; 2]; n],
            ref_idx_l0: vec![-1; n],
            ref_poc_l0: vec![-1; n],
            ref_poc_l1: vec![-1; n],
            mv_l1: vec![[0; 2]; n],
            ref_idx_l1: vec![-1; n],
            mb_field: Vec::new(),
        }
    }
}

/// Picture metadata fixed when a picture is created.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PictureMeta {
    /// Coded (MB-aligned) width of the planes.
    pub(crate) width: u32,
    /// Coded (MB-aligned) height of the planes.
    pub(crate) height: u32,
    pub(crate) frame_num: u32,
    pub(crate) pic_order_cnt: i32,
    /// TopFieldOrderCnt and BottomFieldOrderCnt (both the POC for a field)
    pub(crate) field_poc: [i32; 2],
    pub(crate) mb_width: u32,
    pub(crate) is_intra: bool,
    pub(crate) structure: PictureStructure,
    /// 4:2:2 chroma planes (full height) instead of 4:2:0
    pub(crate) chroma_422: bool,
    /// Bytes per sample in the planes: 1 for 8-bit, 2 for 10-bit
    pub(crate) sample_bytes: u8,
}

/// A decoded picture in the DPB, shared via Arc.
///
/// Usually complete when created. The frame-threaded decoder also inserts
/// pictures that are still being decoded: their planes become readable one
/// macroblock row at a time (see `picture_sync`). Readers therefore go
/// through accessors that expose only published rows, and wait for the rows
/// they need first.
pub struct DecodedPicture {
    pub width: u32,
    pub height: u32,
    pub frame_num: u32,
    pub pic_order_cnt: i32,
    /// TopFieldOrderCnt and BottomFieldOrderCnt, used by MBAFF field MBs
    /// (implicit weights, direct-mode co-located field choice)
    pub field_poc: [i32; 2],
    /// Width in macroblocks (for indexing into the motion data).
    pub mb_width: u32,
    /// Whether this picture is intra-only (all MBs are intra).
    pub is_intra: bool,
    /// Whether this decoded picture is a frame, top field, or bottom field.
    pub structure: PictureStructure,
    /// 4:2:2 chroma planes (full height) instead of 4:2:0
    pub(crate) chroma_422: bool,
    /// Bytes per sample in the planes: 1 for 8-bit, 2 for 10-bit
    pub(crate) sample_bytes: u8,
    /// Y, Cb, Cr planes in coded layout (stride = width, width / 2, in
    /// samples).
    planes: [SharedPlane; 3],
    /// Final macroblock rows of the planes.
    progress: RowProgress,
    /// Set before the last row is published.
    motion: OnceLock<Motion>,
    /// For a PAFF view (see `field_view`, `pair_frame`), the picture(s) it
    /// was made from; direct mode reads the co-located motion there
    pub(crate) view_source: Option<ViewSource>,
}

/// The decoded picture(s) behind a PAFF view.
pub(crate) enum ViewSource {
    /// A frame view of a complementary field pair: [top, bottom]
    Pair([Arc<DecodedPicture>; 2]),
    /// A field view of a decoded frame
    FieldOf(Arc<DecodedPicture>),
}

impl core::fmt::Debug for DecodedPicture {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DecodedPicture")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("frame_num", &self.frame_num)
            .field("pic_order_cnt", &self.pic_order_cnt)
            .field("structure", &self.structure)
            .field("rows", &(self.progress.rows(), self.progress.total()))
            .finish()
    }
}

impl DecodedPicture {
    fn with_planes(meta: PictureMeta, planes: [SharedPlane; 3], progress: RowProgress) -> Self {
        Self {
            width: meta.width,
            height: meta.height,
            frame_num: meta.frame_num,
            pic_order_cnt: meta.pic_order_cnt,
            field_poc: meta.field_poc,
            mb_width: meta.mb_width,
            is_intra: meta.is_intra,
            structure: meta.structure,
            chroma_422: meta.chroma_422,
            sample_bytes: meta.sample_bytes,
            planes,
            progress,
            motion: OnceLock::new(),
            view_source: None,
        }
    }

    /// Macroblock rows covered by a luma plane of `len` bytes.
    fn mb_rows(width: u32, sample_bytes: u8, len: usize) -> u32 {
        let row_bytes = width as usize * 16 * sample_bytes as usize;
        if row_bytes == 0 {
            0
        } else {
            len.div_ceil(row_bytes) as u32
        }
    }

    /// Field `bottom` of a decoded frame, as a separate picture for field
    /// pictures referencing it: every other line, the field's POC. Motion
    /// data is not carried over (direct mode treats it as intra).
    pub(crate) fn field_view(self: &Arc<Self>, bottom: bool) -> DecodedPicture {
        self.wait_complete();
        let lines = |plane: &[u8], stride: usize| -> Vec<u8> {
            plane
                .chunks(stride.max(1))
                .skip(bottom as usize)
                .step_by(2)
                .flatten()
                .copied()
                .collect()
        };
        let w = self.row_bytes();
        let poc = self.field_poc[bottom as usize];
        let meta = PictureMeta {
            width: self.width,
            height: self.height / 2,
            frame_num: self.frame_num,
            pic_order_cnt: poc,
            field_poc: [poc, poc],
            mb_width: self.mb_width,
            is_intra: self.is_intra,
            chroma_422: self.chroma_422,
            sample_bytes: self.sample_bytes,
            structure: if bottom {
                PictureStructure::BottomField
            } else {
                PictureStructure::TopField
            },
        };
        let total_mbs = self.mb_width as usize * (self.height as usize / 32);
        let mut view = DecodedPicture::new_complete(
            meta,
            lines(self.luma(), w),
            lines(self.chroma(0), w / 2),
            lines(self.chroma(1), w / 2),
            Motion::unavailable(total_mbs),
        );
        view.view_source = Some(ViewSource::FieldOf(self.clone()));
        view
    }

    /// The frame of a complementary field pair, for frame pictures
    /// referencing it: the fields' lines interleaved, POC the smaller of
    /// theirs. Motion data is not carried over.
    pub(crate) fn pair_frame(
        top: &Arc<DecodedPicture>,
        bottom: &Arc<DecodedPicture>,
    ) -> DecodedPicture {
        top.wait_complete();
        bottom.wait_complete();
        let interleave = |t: &[u8], b: &[u8], stride: usize| -> Vec<u8> {
            let stride = stride.max(1);
            let mut out = Vec::with_capacity(t.len() + b.len());
            for (lt, lb) in t.chunks(stride).zip(b.chunks(stride)) {
                out.extend_from_slice(lt);
                out.extend_from_slice(lb);
            }
            out
        };
        let w = top.row_bytes();
        let meta = PictureMeta {
            width: top.width,
            height: top.height * 2,
            frame_num: top.frame_num,
            pic_order_cnt: top.pic_order_cnt.min(bottom.pic_order_cnt),
            field_poc: [top.pic_order_cnt, bottom.pic_order_cnt],
            mb_width: top.mb_width,
            is_intra: top.is_intra && bottom.is_intra,
            chroma_422: top.chroma_422,
            sample_bytes: top.sample_bytes,
            structure: PictureStructure::Frame,
        };
        let total_mbs = top.mb_width as usize * (top.height as usize / 8);
        let mut view = DecodedPicture::new_complete(
            meta,
            interleave(top.luma(), bottom.luma(), w),
            interleave(top.chroma(0), bottom.chroma(0), w / 2),
            interleave(top.chroma(1), bottom.chroma(1), w / 2),
            Motion::unavailable(total_mbs),
        );
        view.view_source = Some(ViewSource::Pair([top.clone(), bottom.clone()]));
        view
    }

    /// A fully decoded picture.
    pub(crate) fn new_complete(
        meta: PictureMeta,
        y: Vec<u8>,
        u: Vec<u8>,
        v: Vec<u8>,
        motion: Motion,
    ) -> Self {
        let rows = Self::mb_rows(meta.width, meta.sample_bytes, y.len());
        let pic = Self::with_planes(
            meta,
            [
                SharedPlane::from_vec(y),
                SharedPlane::from_vec(u),
                SharedPlane::from_vec(v),
            ],
            RowProgress::complete(rows),
        );
        let _ = pic.motion.set(motion);
        pic
    }

    /// A picture that will be filled in by the returned writer. Its planes
    /// are coded size; readers block until the rows they need are published.
    #[cfg_attr(not(feature = "std"), allow(dead_code))]
    pub(crate) fn new_in_progress(meta: PictureMeta) -> (Arc<Self>, PictureWriter) {
        let luma_len = meta.width as usize * meta.height as usize * meta.sample_bytes as usize;
        let chroma_len = if meta.chroma_422 {
            luma_len / 2
        } else {
            luma_len / 4
        };
        let rows = Self::mb_rows(meta.width, meta.sample_bytes, luma_len);
        let pic = Arc::new(Self::with_planes(
            meta,
            [
                SharedPlane::from_vec(vec![0; luma_len]),
                SharedPlane::from_vec(vec![0; chroma_len]),
                SharedPlane::from_vec(vec![0; chroma_len]),
            ],
            RowProgress::start(rows),
        ));
        let writer = PictureWriter {
            pic: Arc::clone(&pic),
            rows: 0,
            done: false,
        };
        (pic, writer)
    }

    /// Bytes of plane `p` (0 = Y, 1 = Cb, 2 = Cr) covered by `rows` MB rows.
    #[inline]
    fn plane_bytes(&self, p: usize, rows: u32) -> usize {
        let len = self.planes[p].len();
        if rows >= self.progress.total() {
            return len;
        }
        let (w, h) = if p == 0 {
            (self.row_bytes(), 16)
        } else {
            (self.row_bytes() / 2, self.chroma_mb_rows())
        };
        (rows as usize * h * w).min(len)
    }

    /// Published part of plane `p` (the whole plane once complete).
    #[inline]
    fn plane(&self, p: usize) -> &[u8] {
        let len = self.plane_bytes(p, self.progress.rows());
        // SAFETY: `len` covers only published rows, which are never written
        // again (see `picture_sync`).
        unsafe { self.planes[p].prefix(len) }
    }

    /// Published part of the luma plane.
    #[inline]
    pub(crate) fn luma(&self) -> &[u8] {
        self.plane(0)
    }

    /// Published part of a chroma plane (0 = Cb, 1 = Cr).
    #[inline]
    pub(crate) fn chroma(&self, plane: usize) -> &[u8] {
        self.plane(1 + plane.min(1))
    }

    /// Full length of the luma plane, published or not.
    #[inline]
    pub(crate) fn luma_len(&self) -> usize {
        self.planes[0].len()
    }

    /// Full length of a chroma plane, published or not.
    #[inline]
    pub(crate) fn chroma_len(&self) -> usize {
        self.planes[1].len()
    }

    /// Block until luma bytes `0..end` are final.
    #[inline]
    pub(crate) fn wait_luma(&self, end: usize) {
        self.wait_plane_bytes(self.row_bytes(), 16, end);
    }

    /// Block until chroma bytes `0..end` (of both chroma planes) are final.
    #[inline]
    pub(crate) fn wait_chroma(&self, end: usize) {
        self.wait_plane_bytes(self.row_bytes() / 2, self.chroma_mb_rows(), end);
    }

    /// Bytes per luma row.
    #[inline]
    pub(crate) fn row_bytes(&self) -> usize {
        self.width as usize * self.sample_bytes as usize
    }

    /// Chroma lines per macroblock row: 8 for 4:2:0, 16 for 4:2:2.
    #[inline]
    pub(crate) fn chroma_mb_rows(&self) -> usize {
        if self.chroma_422 {
            16
        } else {
            8
        }
    }

    #[inline]
    fn wait_plane_bytes(&self, stride: usize, lines_per_row: usize, end: usize) {
        if stride == 0 || end == 0 {
            return;
        }
        let lines = end.div_ceil(stride);
        let rows = lines.div_ceil(lines_per_row);
        self.progress.wait(rows.min(u32::MAX as usize) as u32);
    }

    /// Block until the whole picture is decoded.
    #[inline]
    pub(crate) fn wait_complete(&self) {
        self.progress.wait(self.progress.total());
    }

    /// Motion data; blocks until the picture is completely decoded.
    pub(crate) fn motion(&self) -> &Motion {
        self.wait_complete();
        self.motion
            .get()
            .expect("motion data is set before the picture completes")
    }

    /// Reference to chroma plane `plane` (0 = Cb, 1 = Cr) starting at byte
    /// `offset`, for [`chroma_mc`](crate::inter_pred::chroma_mc).
    #[inline]
    pub(crate) fn chroma_ref(&self, plane: usize, offset: usize) -> ChromaRef<'_> {
        ChromaRef {
            pic: self,
            plane,
            offset,
        }
    }
}

/// A chroma plane of a reference picture from a byte offset (field-aware
/// motion compensation starts at the field's first line). Motion
/// compensation waits for the rows it reads before touching the data.
#[derive(Clone, Copy)]
pub(crate) struct ChromaRef<'a> {
    pub(crate) pic: &'a DecodedPicture,
    pub(crate) plane: usize,
    pub(crate) offset: usize,
}

/// The unique writer of an in-progress [`DecodedPicture`]. Copies final rows
/// from the decoder's private planes into the shared picture and publishes
/// them. Dropping it unfinished (error or panic) completes the picture with
/// mid-grey content so readers never wait forever.
#[cfg_attr(not(feature = "std"), allow(dead_code))]
pub(crate) struct PictureWriter {
    pic: Arc<DecodedPicture>,
    rows: u32,
    done: bool,
}

#[cfg_attr(not(feature = "std"), allow(dead_code))]
impl PictureWriter {
    /// Copy MB rows `self.rows..upto` from the private coded planes and
    /// publish them. Never publishes the last row; use [`finish`](Self::finish).
    pub(crate) fn publish(&mut self, y: &[u8], u: &[u8], v: &[u8], upto: u32) {
        let upto = upto.min(self.pic.progress.total().saturating_sub(1));
        if upto <= self.rows {
            return;
        }
        self.copy_rows(y, u, v, upto);
        self.pic.progress.publish(upto);
    }

    /// Copy every remaining row, attach the motion data and mark the picture
    /// complete.
    pub(crate) fn finish(mut self, y: &[u8], u: &[u8], v: &[u8], motion: Motion) {
        let total = self.pic.progress.total();
        self.copy_rows(y, u, v, total);
        let _ = self.pic.motion.set(motion);
        self.pic.progress.publish(total);
        self.done = true;
    }

    fn copy_rows(&mut self, y: &[u8], u: &[u8], v: &[u8], upto: u32) {
        for (p, src) in [y, u, v].into_iter().enumerate() {
            let from = self.pic.plane_bytes(p, self.rows);
            let to = self.pic.plane_bytes(p, upto).min(src.len()).max(from);
            // SAFETY: this is the unique writer; `from..to` lies at or beyond
            // the published length and within the plane.
            unsafe { self.pic.planes[p].write(from, &src[from..to]) };
        }
        self.rows = upto;
    }

    /// Complete the picture with mid-grey for every unpublished byte and
    /// empty motion, so pictures referencing it can still be decoded.
    fn abandon(&mut self) {
        let total = self.pic.progress.total();
        for p in 0..3 {
            let from = self.pic.plane_bytes(p, self.rows);
            let len = self.pic.planes[p].len() - from;
            // SAFETY: unique writer; the range is unpublished and in bounds.
            unsafe { self.pic.planes[p].fill(from, len, 128) };
        }
        let total_mbs = self.pic.mb_width as usize * total as usize;
        let _ = self.pic.motion.set(Motion::unavailable(total_mbs));
        self.rows = total;
        self.pic.progress.publish(total);
        self.done = true;
    }
}

impl Drop for PictureWriter {
    fn drop(&mut self) {
        if !self.done {
            self.abandon();
        }
    }
}

/// A DPB entry wrapping an Arc<DecodedPicture> with mutable status.
struct DpbEntry {
    pic: Arc<DecodedPicture>,
    reference: ReferenceStatus,
    /// A frame's two fields, for field pictures referencing it (PAFF)
    field_views: core::cell::OnceCell<[Arc<DecodedPicture>; 2]>,
    /// The frame of a complementary field pair, cached on the pair's
    /// second field, for frame pictures referencing it (PAFF)
    frame_view: core::cell::OnceCell<Arc<DecodedPicture>>,
}

/// The Decoded Picture Buffer.
pub struct Dpb {
    max_ref_frames: usize,
    entries: Vec<DpbEntry>,
    // POC type 0 state
    prev_poc_msb: i32,
    prev_poc_lsb: u32,
    // POC types 1 and 2 state: frame_num and FrameNumOffset of the previous
    // picture (spec 8.2.1.2)
    prev_frame_num: u32,
    prev_frame_num_offset: i32,
    /// PrevRefFrameNum (spec 7.4.3): frame_num of the last reference picture,
    /// `None` before the first one
    prev_ref_frame_num: Option<u32>,
}

impl Dpb {
    pub fn new(max_ref_frames: usize) -> Self {
        Self {
            max_ref_frames,
            entries: Vec::new(),
            prev_poc_msb: 0,
            prev_poc_lsb: 0,
            prev_frame_num: 0,
            prev_frame_num_offset: 0,
            prev_ref_frame_num: None,
        }
    }

    /// Update capacity when a new SPS is parsed.
    pub fn set_max_ref_frames(&mut self, max_ref_frames: u32) {
        self.max_ref_frames = max_ref_frames as usize;
    }

    /// Clear the entire DPB. Called on IDR.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.prev_poc_msb = 0;
        self.prev_poc_lsb = 0;
        self.prev_frame_num = 0;
        self.prev_frame_num_offset = 0;
    }

    /// Record the frame_num a reference picture leaves as PrevRefFrameNum
    /// (0 for a picture with MMCO 5).
    pub(crate) fn set_prev_ref_frame_num(&mut self, frame_num: u32) {
        self.prev_ref_frame_num = Some(frame_num);
    }

    /// Decoding process for gaps in frame_num (spec 8.2.5.2): when a
    /// picture's frame_num skips values after PrevRefFrameNum, each skipped
    /// value becomes a "non-existing" short-term reference frame, inserted
    /// through the sliding window so that it pushes older references out.
    /// They are never output. As FFmpeg does, each holds a copy of the latest
    /// short-term reference with its POC advanced by 2, in case a stream
    /// does reference one. Only the last `max_num_ref_frames` of a long gap
    /// can survive the sliding window, so earlier ones are skipped.
    pub(crate) fn fill_frame_num_gap(&mut self, frame_num: u32, max_frame_num: u32) {
        let Some(prev) = self.prev_ref_frame_num else {
            return;
        };
        // prev is out of range only when the SPS changed without an IDR
        // (a broken stream); there is no gap to fill then.
        if max_frame_num == 0
            || frame_num >= max_frame_num
            || prev >= max_frame_num
            || frame_num == prev
            || frame_num == (prev + 1) % max_frame_num
        {
            return;
        }
        let missing = (frame_num + max_frame_num - prev - 1) % max_frame_num;
        let skip = missing.saturating_sub(self.max_ref_frames.max(1) as u32);
        let mut unused = (prev + 1 + skip) % max_frame_num;
        while unused != frame_num {
            let Some(latest) = self
                .short_term_ref_list(unused, max_frame_num, false, false)
                .into_iter()
                .find(|p| p.structure == PictureStructure::Frame)
            else {
                break;
            };
            latest.wait_complete();
            let meta = PictureMeta {
                width: latest.width,
                height: latest.height,
                frame_num: unused,
                pic_order_cnt: latest.pic_order_cnt.wrapping_add(2),
                field_poc: latest.field_poc.map(|p| p.wrapping_add(2)),
                mb_width: latest.mb_width,
                is_intra: true,
                structure: PictureStructure::Frame,
                chroma_422: latest.chroma_422,
                sample_bytes: latest.sample_bytes,
            };
            let total_mbs = latest.mb_width as usize * (latest.height as usize / 16);
            let pic = DecodedPicture::new_complete(
                meta,
                latest.luma().to_vec(),
                latest.chroma(0).to_vec(),
                latest.chroma(1).to_vec(),
                Motion::unavailable(total_mbs),
            );
            self.insert(Arc::new(pic), ReferenceStatus::ShortTerm);
            unused = (unused + 1) % max_frame_num;
        }
        self.prev_ref_frame_num = Some((frame_num + max_frame_num - 1) % max_frame_num);
    }

    /// Insert a decoded picture into the DPB.
    /// Applies sliding window marking if needed (spec 8.2.5.3).
    pub fn insert(&mut self, pic: Arc<DecodedPicture>, reference: ReferenceStatus) {
        self.insert_marked(pic, reference, true);
    }

    /// Insert a picture; `sliding_window` is false when its slice header
    /// marks references adaptively (MMCO), which replaces the sliding window.
    pub(crate) fn insert_marked(
        &mut self,
        pic: Arc<DecodedPicture>,
        reference: ReferenceStatus,
        sliding_window: bool,
    ) {
        // Sliding window only applies to short-term references (spec 8.2.5.3)
        if sliding_window && reference == ReferenceStatus::ShortTerm {
            // Don't evict the complementary field of the picture being inserted.
            // Two fields with the same frame_num form a pair and count as one
            // reference frame (spec 7.4.3.1). Skip eviction if the new field
            // completes an existing pair.
            let completes_pair = pic.structure != PictureStructure::Frame
                && self.entries.iter().any(|e| {
                    e.reference == ReferenceStatus::ShortTerm
                        && e.pic.frame_num == pic.frame_num
                        && e.pic.structure != pic.structure
                        && e.pic.structure != PictureStructure::Frame
                });
            if !completes_pair {
                self.sliding_window_mark();
            }
        }
        self.entries.push(DpbEntry {
            pic,
            reference,
            field_views: core::cell::OnceCell::new(),
            frame_view: core::cell::OnceCell::new(),
        });
        self.remove_unused();
    }

    /// Get the list of short-term reference pictures, sorted by descending PicNum.
    /// Used to build ref_pic_list_0 for P slices (spec 8.2.4.2.1 / 8.2.4.2.5).
    ///
    /// The order is by descending FrameNumWrap (spec 8.2.4.1): `frame_num`, less
    /// `max_frame_num` for pictures whose frame_num is above the current
    /// picture's `frame_num`. That is decoding order, which isn't display (POC)
    /// order once B-pictures are used as references.
    ///
    /// For field pictures (`is_field_pic=true`): field ordering per spec 8.2.4.2.5 —
    /// group fields by frame, frames by descending FrameNumWrap, and within each
    /// frame emit the same-parity field before the opposite-parity field.
    pub fn short_term_ref_list(
        &self,
        frame_num: u32,
        max_frame_num: u32,
        is_field_pic: bool,
        bottom_field_flag: bool,
    ) -> Vec<Arc<DecodedPicture>> {
        let mut refs: Vec<_> = self.short_term_refs(is_field_pic);

        let frame_num_wrap = |p: &DecodedPicture| {
            if p.frame_num > frame_num {
                i64::from(p.frame_num) - i64::from(max_frame_num)
            } else {
                i64::from(p.frame_num)
            }
        };
        if is_field_pic {
            // Spec 8.2.4.2.2 / 8.2.4.2.5: frames by descending FrameNumWrap,
            // then long-term frames by LongTermFrameIdx, each split into
            // fields of alternating parity
            let mut frames = Self::field_groups(refs);
            frames.sort_by_key(|f| core::cmp::Reverse(frame_num_wrap(&f[0])));
            let mut list = Self::alternate_parity(&frames, bottom_field_flag);
            list.extend(self.long_term_fields(bottom_field_flag));
            return list;
        }
        refs.sort_by_key(|p| core::cmp::Reverse(frame_num_wrap(p)));
        // Append long-term refs sorted by ascending long_term_frame_idx (spec 8.2.4.2.1)
        refs.extend(self.long_term_ref_list());
        refs
    }

    /// Group reference fields into the frames (complementary field pairs)
    /// they belong to: fields share a frame_num; a frame entry is its own
    /// group. Groups keep the order of their first field.
    fn field_groups(pics: Vec<Arc<DecodedPicture>>) -> Vec<Vec<Arc<DecodedPicture>>> {
        let mut groups: Vec<Vec<Arc<DecodedPicture>>> = Vec::new();
        for p in pics {
            let pair = groups.iter_mut().find(|g| {
                p.structure != PictureStructure::Frame
                    && g[0].structure != PictureStructure::Frame
                    && g[0].frame_num == p.frame_num
            });
            match pair {
                Some(g) => g.push(p),
                None => groups.push(vec![p]),
            }
        }
        groups
    }

    /// Fields of `frames` (in frame order) alternating in parity, starting
    /// with the current field's: one cursor takes the next frame holding a
    /// reference field of the current parity, the other the next holding
    /// one of the other parity; when one runs out the other continues
    /// (spec 8.2.4.2.5). A frame entry provides both parities.
    fn alternate_parity(
        frames: &[Vec<Arc<DecodedPicture>>],
        bottom_field_flag: bool,
    ) -> Vec<Arc<DecodedPicture>> {
        let field = |f: &Vec<Arc<DecodedPicture>>, bottom: bool| {
            f.iter()
                .find(|p| match p.structure {
                    PictureStructure::Frame => true,
                    PictureStructure::BottomField => bottom,
                    PictureStructure::TopField => !bottom,
                })
                .cloned()
        };
        let same: Vec<_> = frames
            .iter()
            .filter_map(|f| field(f, bottom_field_flag))
            .collect();
        let other: Vec<_> = frames
            .iter()
            .filter_map(|f| field(f, !bottom_field_flag))
            .collect();
        let mut out = Vec::with_capacity(same.len() + other.len());
        let (mut a, mut b) = (same.into_iter(), other.into_iter());
        loop {
            let (x, y) = (a.next(), b.next());
            if x.is_none() && y.is_none() {
                break;
            }
            out.extend(x);
            out.extend(y);
        }
        out
    }

    /// Long-term reference fields for a field picture's lists: frames by
    /// ascending LongTermFrameIdx, split into alternating-parity fields.
    fn long_term_fields(&self, bottom_field_flag: bool) -> Vec<Arc<DecodedPicture>> {
        let mut lt = self.long_term_refs(true);
        lt.sort_by_key(|(_, idx)| *idx);
        let mut frames: Vec<Vec<Arc<DecodedPicture>>> = Vec::new();
        let mut idxs: Vec<u32> = Vec::new();
        for (p, idx) in lt {
            match idxs.iter().position(|&i| i == idx) {
                Some(k) => frames[k].push(p),
                None => {
                    idxs.push(idx);
                    frames.push(vec![p]);
                }
            }
        }
        Self::alternate_parity(&frames, bottom_field_flag)
    }

    /// B-slice field list (spec 8.2.4.2.4 / 8.2.4.2.5): frames ordered by
    /// the POC of their reference fields (the smaller when both are), those
    /// not after the current field first (`after_first` reverses that, for
    /// list 1), split into alternating-parity fields, then the long-term
    /// fields.
    fn b_field_list(
        &self,
        current_poc: i32,
        bottom_field_flag: bool,
        after_first: bool,
    ) -> Vec<Arc<DecodedPicture>> {
        let short_term: Vec<_> = self.short_term_refs(true);
        let frames = Self::field_groups(short_term);
        let poc =
            |f: &Vec<Arc<DecodedPicture>>| f.iter().map(|p| p.pic_order_cnt).min().unwrap_or(0);
        let mut before: Vec<_> = frames
            .iter()
            .filter(|f| poc(f) <= current_poc)
            .cloned()
            .collect();
        before.sort_by_key(|f| core::cmp::Reverse(poc(f)));
        let mut after: Vec<_> = frames
            .iter()
            .filter(|f| poc(f) > current_poc)
            .cloned()
            .collect();
        after.sort_by_key(|f| poc(f));
        let ordered: Vec<_> = if after_first {
            after.into_iter().chain(before).collect()
        } else {
            before.into_iter().chain(after).collect()
        };
        let mut list = Self::alternate_parity(&ordered, bottom_field_flag);
        list.extend(self.long_term_fields(bottom_field_flag));
        list
    }

    /// Reference pictures with status `status` (ShortTerm, or any
    /// LongTerm), as the current picture structure sees them, with their
    /// LongTermFrameIdx. A field picture sees a frame as its two fields; a
    /// frame picture sees a complementary reference field pair as one frame
    /// and cannot use a lone reference field (spec 8.2.4.2.1, 8.2.4.2.2). The
    /// views are built once per DPB entry, so repeated calls return the same
    /// pictures.
    fn ref_pictures(&self, long_term: bool, is_field_pic: bool) -> Vec<(Arc<DecodedPicture>, u32)> {
        let status = |e: &DpbEntry| match e.reference {
            ReferenceStatus::ShortTerm if !long_term => Some(0),
            ReferenceStatus::LongTerm(idx) if long_term => Some(idx),
            _ => None,
        };
        let mut out = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            let Some(idx) = status(e) else { continue };
            let is_frame = e.pic.structure == PictureStructure::Frame;
            if is_field_pic {
                if is_frame {
                    let views = e.field_views.get_or_init(|| {
                        [false, true].map(|bottom| Arc::new(e.pic.field_view(bottom)))
                    });
                    out.extend(views.iter().map(|v| (v.clone(), idx)));
                } else {
                    out.push((e.pic.clone(), idx));
                }
            } else if is_frame {
                out.push((e.pic.clone(), idx));
            } else {
                // The first field of a complete pair, earlier in the DPB
                let partner = self.entries[..i].iter().find(|o| {
                    o.pic.structure != PictureStructure::Frame
                        && o.pic.structure != e.pic.structure
                        && o.pic.frame_num == e.pic.frame_num
                        && status(o) == Some(idx)
                });
                if let Some(o) = partner {
                    let (top, bottom) = if e.pic.structure == PictureStructure::TopField {
                        (&e.pic, &o.pic)
                    } else {
                        (&o.pic, &e.pic)
                    };
                    let frame = e
                        .frame_view
                        .get_or_init(|| Arc::new(DecodedPicture::pair_frame(top, bottom)));
                    out.push((frame.clone(), idx));
                }
            }
        }
        out
    }

    /// Every long-term reference picture with its LongTermFrameIdx, as the
    /// current picture structure sees them (see `ref_pictures`).
    pub fn long_term_refs(&self, is_field_pic: bool) -> Vec<(Arc<DecodedPicture>, u32)> {
        self.ref_pictures(true, is_field_pic)
    }

    /// Every short-term reference picture, as the current picture
    /// structure sees them, in no particular order.
    pub fn short_term_refs(&self, is_field_pic: bool) -> Vec<Arc<DecodedPicture>> {
        self.ref_pictures(false, is_field_pic)
            .into_iter()
            .map(|(p, _)| p)
            .collect()
    }

    /// Build ref_pic_list_0 for B slices (spec 8.2.4.2.3 / 8.2.4.2.5).
    /// Short-term refs with POC < current sorted by descending POC,
    /// then refs with POC > current sorted by ascending POC.
    /// For field pictures, additionally group by frame_num with same-parity first.
    pub fn ref_list_l0_b(
        &self,
        current_poc: i32,
        is_field_pic: bool,
        bottom_field_flag: bool,
    ) -> Vec<Arc<DecodedPicture>> {
        if is_field_pic {
            return self.b_field_list(current_poc, bottom_field_flag, false);
        }
        let short_term: Vec<_> = self.short_term_refs(false);

        let mut before: Vec<_> = short_term
            .iter()
            .filter(|p| p.pic_order_cnt <= current_poc)
            .cloned()
            .collect();
        before.sort_by_key(|a| core::cmp::Reverse(a.pic_order_cnt)); // descending

        let mut after: Vec<_> = short_term
            .iter()
            .filter(|p| p.pic_order_cnt > current_poc)
            .cloned()
            .collect();
        after.sort_by_key(|a| a.pic_order_cnt); // ascending

        before.extend(after);

        // Append long-term refs (spec 8.2.4.2.3)
        before.extend(self.long_term_ref_list());
        before
    }

    /// Build ref_pic_list_1 for B slices (spec 8.2.4.2.4 / 8.2.4.2.5).
    /// Short-term refs with POC > current sorted by ascending POC,
    /// then refs with POC <= current sorted by descending POC.
    /// If L1 == L0 and has more than one entry, swap the first two.
    pub fn ref_list_l1_b(
        &self,
        current_poc: i32,
        is_field_pic: bool,
        bottom_field_flag: bool,
    ) -> Vec<Arc<DecodedPicture>> {
        let mut after = if is_field_pic {
            self.b_field_list(current_poc, bottom_field_flag, true)
        } else {
            self.frame_list_l1_b(current_poc)
        };

        // Spec 8.2.4.2.4: if L1 == L0 and has more than one entry, swap first two
        let l0 = self.ref_list_l0_b(current_poc, is_field_pic, bottom_field_flag);
        if after.len() > 1
            && after.len() == l0.len()
            && after.iter().zip(l0.iter()).all(|(a, b)| Arc::ptr_eq(a, b))
        {
            after.swap(0, 1);
        }

        after
    }

    /// Frame-picture list 1 before the L0 comparison (spec 8.2.4.2.3).
    fn frame_list_l1_b(&self, current_poc: i32) -> Vec<Arc<DecodedPicture>> {
        let short_term: Vec<_> = self.short_term_refs(false);

        let mut after: Vec<_> = short_term
            .iter()
            .filter(|p| p.pic_order_cnt > current_poc)
            .cloned()
            .collect();
        after.sort_by_key(|a| a.pic_order_cnt); // ascending

        let mut before: Vec<_> = short_term
            .iter()
            .filter(|p| p.pic_order_cnt <= current_poc)
            .cloned()
            .collect();
        before.sort_by_key(|a| core::cmp::Reverse(a.pic_order_cnt)); // descending

        after.extend(before);

        // Append long-term refs (spec 8.2.4.2.4)
        after.extend(self.long_term_ref_list());
        after
    }

    /// Apply ref_pic_list_modification to reorder a reference list (spec 8.2.4.3).
    /// `ops`: list of (modification_of_pic_nums_idc, abs_diff_pic_num_minus1)
    /// `curr_pic_num`: CurrPicNum (= frame_num for frames, 2*frame_num+1 for fields)
    /// `max_pic_num`: MaxPicNum (= MaxFrameNum for frames, 2*MaxFrameNum for fields)
    /// `is_field_pic`: true for field pictures (PicNum = 2*frame_num + parity_bit)
    /// `bottom_field_flag`: current field parity (only meaningful when is_field_pic)
    /// `short_term`: every short-term reference picture. A command can name one
    /// that the initial list had no room for, or that an earlier command pushed
    /// out of it (spec 8.2.4.3.1 picks from all short-term references).
    /// `long_term`: every long-term reference with its LongTermFrameIdx, for
    /// commands naming a LongTermPicNum (spec 8.2.4.3.2).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_ref_list_modification(
        ref_list: &mut Vec<Arc<DecodedPicture>>,
        ops: &[(u32, u32)],
        curr_pic_num: u32,
        max_pic_num: u32,
        is_field_pic: bool,
        bottom_field_flag: bool,
        short_term: &[Arc<DecodedPicture>],
        long_term: &[(Arc<DecodedPicture>, u32)],
    ) {
        if ops.is_empty() || ref_list.is_empty() {
            return;
        }
        let num_active = ref_list.len();
        let mut pred_pic_num = curr_pic_num;
        let mut ref_idx_lx = 0usize;

        for &(idc, val) in ops {
            // Per spec 8.2.4.3.1, ref_idx_lx must stay in [0, num_active - 1].
            // A malformed bitstream may encode more RPLM commands than there
            // are slots — bail out instead of indexing out of bounds.
            if ref_idx_lx >= num_active {
                return;
            }
            if idc == 2 {
                // Long-term reference by LongTermPicNum (spec 8.2.4.3.2):
                // LongTermFrameIdx for frames, 2 * idx + 1 for a field of
                // the current parity and 2 * idx for the other
                let named = long_term.iter().find(|(p, idx)| {
                    let lt_pic_num = if is_field_pic {
                        let same_parity =
                            (p.structure == PictureStructure::BottomField) == bottom_field_flag;
                        2 * idx + same_parity as u32
                    } else {
                        *idx
                    };
                    lt_pic_num == val
                });
                if let Some((pic, _)) = named {
                    Self::insert_at(ref_list, ref_idx_lx, pic, num_active);
                }
                ref_idx_lx += 1;
                continue;
            }
            if idc > 1 {
                continue; // idc=3 terminates the loop (handled by caller)
            }
            let abs_diff = val + 1;
            // Spec 8.2.4.3.1: modular arithmetic on pic_num. Use wrapping
            // ops to handle malformed streams where abs_diff exceeds the
            // valid range without panicking.
            let pic_num = if idc == 0 {
                if pred_pic_num >= abs_diff {
                    pred_pic_num - abs_diff
                } else {
                    pred_pic_num
                        .wrapping_add(max_pic_num)
                        .wrapping_sub(abs_diff)
                }
            } else {
                let sum = pred_pic_num.wrapping_add(abs_diff);
                if sum >= max_pic_num {
                    sum.wrapping_sub(max_pic_num)
                } else {
                    sum
                }
            };
            pred_pic_num = pic_num;

            // Spec 8.2.4.3.1: find the picture, shift entries right to make room,
            // insert at ref_idx_lx, then remove duplicates after ref_idx_lx.
            // For field pictures, PicNum = 2*frame_num + parity_bit (spec 8.2.4.1).
            let pic_num_match = |p: &DecodedPicture| -> bool {
                if is_field_pic {
                    let p_is_bottom = p.structure == PictureStructure::BottomField;
                    let same_parity = p_is_bottom == bottom_field_flag;
                    let p_pic_num = p.frame_num * 2 + if same_parity { 1 } else { 0 };
                    p_pic_num == pic_num
                } else {
                    p.frame_num == pic_num
                }
            };
            if let Some(pic) = short_term.iter().find(|p| pic_num_match(p)) {
                Self::insert_at(ref_list, ref_idx_lx, pic, num_active);
            }

            ref_idx_lx += 1;
        }
    }

    /// Put `pic` at `ref_idx` of a list of `num_active` entries, shifting the
    /// rest right, and drop its later copies (spec 8.2.4.3.1 and 8.2.4.3.2,
    /// where PicNumF / LongTermPicNumF identify the same picture).
    fn insert_at(
        ref_list: &mut Vec<Arc<DecodedPicture>>,
        ref_idx: usize,
        pic: &Arc<DecodedPicture>,
        num_active: usize,
    ) {
        ref_list.insert(ref_idx, pic.clone());
        let mut n = ref_idx + 1;
        for c in ref_idx + 1..ref_list.len() {
            if !Arc::ptr_eq(&ref_list[c], pic) {
                ref_list.swap(n, c);
                n += 1;
            }
        }
        ref_list.truncate(num_active);
    }

    /// Compute Picture Order Count for the current picture (spec 8.2.1).
    /// TopFieldOrderCnt and BottomFieldOrderCnt of a frame from its
    /// PicOrderCnt (their minimum) and the bottom field's offset; for a field
    /// picture both are its own POC.
    pub(crate) fn frame_field_pocs(sps: &Sps, header: &SliceHeader, poc: i32) -> [i32; 2] {
        if header.field_pic_flag {
            return [poc, poc];
        }
        let bottom_delta = match sps.pic_order_cnt_type {
            0 => header.delta_pic_order_cnt_bottom,
            1 => sps
                .offset_for_top_to_bottom_field
                .wrapping_add(header.delta_pic_order_cnt[1]),
            _ => 0,
        };
        if bottom_delta >= 0 {
            [poc, poc.wrapping_add(bottom_delta)]
        } else {
            [poc.wrapping_sub(bottom_delta), poc]
        }
    }

    pub fn compute_poc(
        &mut self,
        sps: &Sps,
        header: &SliceHeader,
        nal_unit_type: NalUnitType,
        nal_ref_idc: u8,
    ) -> i32 {
        match sps.pic_order_cnt_type {
            0 => self.compute_poc_type0(sps, header, nal_unit_type, nal_ref_idc),
            1 => {
                let offset = self.frame_num_offset(sps, header, nal_unit_type);
                Self::compute_poc_type1(sps, header, offset, nal_ref_idc)
            }
            2 => {
                let offset = self.frame_num_offset(sps, header, nal_unit_type);
                Self::compute_poc_type2(header.frame_num, offset, nal_unit_type, nal_ref_idc)
            }
            _ => 0,
        }
    }

    /// FrameNumOffset of the current picture for POC types 1 and 2 (spec
    /// 8.2.1.2): grows by MaxFrameNum each time frame_num wraps. Records the
    /// current frame_num and offset as the previous picture's (repeated
    /// calls for further slices of the picture give the same result).
    fn frame_num_offset(
        &mut self,
        sps: &Sps,
        header: &SliceHeader,
        nal_unit_type: NalUnitType,
    ) -> i32 {
        let max_frame_num = 1i32 << (sps.log2_max_frame_num_minus4 + 4).min(16);
        let offset = if nal_unit_type == NalUnitType::SliceIdr {
            0
        } else if self.prev_frame_num > header.frame_num {
            self.prev_frame_num_offset.wrapping_add(max_frame_num)
        } else {
            self.prev_frame_num_offset
        };
        self.prev_frame_num = header.frame_num;
        self.prev_frame_num_offset = offset;
        offset
    }

    /// POC type 0 (spec 8.2.1.1) — uses pic_order_cnt_lsb with MSB wrapping.
    fn compute_poc_type0(
        &mut self,
        sps: &Sps,
        header: &SliceHeader,
        nal_unit_type: NalUnitType,
        nal_ref_idc: u8,
    ) -> i32 {
        if nal_unit_type == NalUnitType::SliceIdr {
            self.prev_poc_msb = 0;
            self.prev_poc_lsb = 0;
        }

        let shift = (sps.log2_max_pic_order_cnt_lsb_minus4 + 4).min(31);
        let max_poc_lsb = 1u32 << shift;
        let poc_lsb = header.pic_order_cnt_lsb;

        let poc_msb = if poc_lsb < self.prev_poc_lsb
            && (self.prev_poc_lsb - poc_lsb) >= max_poc_lsb / 2
        {
            self.prev_poc_msb.wrapping_add(max_poc_lsb as i32)
        } else if poc_lsb > self.prev_poc_lsb && (poc_lsb - self.prev_poc_lsb) > max_poc_lsb / 2 {
            self.prev_poc_msb.wrapping_sub(max_poc_lsb as i32)
        } else {
            self.prev_poc_msb
        };

        let top_field_order_cnt = poc_msb.wrapping_add(poc_lsb as i32);
        let poc = if header.field_pic_flag {
            // Field picture: POC is just this field's order count
            if header.bottom_field_flag {
                top_field_order_cnt.wrapping_add(header.delta_pic_order_cnt_bottom)
            } else {
                top_field_order_cnt
            }
        } else {
            // Frame: PicOrderCnt = min(TopFieldOrderCnt, BottomFieldOrderCnt)
            let bottom_field_order_cnt =
                top_field_order_cnt.wrapping_add(header.delta_pic_order_cnt_bottom);
            top_field_order_cnt.min(bottom_field_order_cnt)
        };

        if nal_ref_idc > 0 {
            self.prev_poc_msb = poc_msb;
            self.prev_poc_lsb = poc_lsb;
        }

        poc
    }

    /// POC type 1 (spec 8.2.1.2) — uses delta_pic_order_cnt with cycle offsets.
    /// POC type 1 (spec 8.2.1.2): expected order from the frame_num cycle
    /// in the SPS, plus the coded deltas.
    fn compute_poc_type1(
        sps: &Sps,
        header: &SliceHeader,
        frame_num_offset: i32,
        nal_ref_idc: u8,
    ) -> i32 {
        let cycle_len = sps.num_ref_frames_in_pic_order_cnt_cycle as i32;
        let mut abs_frame_num = if cycle_len != 0 {
            frame_num_offset.wrapping_add(header.frame_num as i32)
        } else {
            0
        };
        if nal_ref_idc == 0 && abs_frame_num > 0 {
            abs_frame_num -= 1;
        }
        let mut expected_poc = 0i32;
        if abs_frame_num > 0 {
            let delta_per_cycle = sps
                .offset_for_ref_frame
                .iter()
                .fold(0i32, |a, &b| a.wrapping_add(b));
            let cycle = (abs_frame_num - 1) / cycle_len;
            let in_cycle = ((abs_frame_num - 1) % cycle_len) as usize;
            expected_poc = cycle.wrapping_mul(delta_per_cycle);
            for &o in sps.offset_for_ref_frame.iter().take(in_cycle + 1) {
                expected_poc = expected_poc.wrapping_add(o);
            }
        }
        if nal_ref_idc == 0 {
            expected_poc = expected_poc.wrapping_add(sps.offset_for_non_ref_pic);
        }

        let top_field_order_cnt = expected_poc.wrapping_add(header.delta_pic_order_cnt[0]);
        let bottom_from_top = |top: i32| {
            top.wrapping_add(sps.offset_for_top_to_bottom_field)
                .wrapping_add(header.delta_pic_order_cnt[1])
        };
        if !header.field_pic_flag {
            top_field_order_cnt.min(bottom_from_top(top_field_order_cnt))
        } else if header.bottom_field_flag {
            // A bottom field codes only delta_pic_order_cnt[0]
            expected_poc
                .wrapping_add(sps.offset_for_top_to_bottom_field)
                .wrapping_add(header.delta_pic_order_cnt[0])
        } else {
            top_field_order_cnt
        }
    }

    /// POC type 2 (spec 8.2.1.3): output order is decoding order.
    fn compute_poc_type2(
        frame_num: u32,
        frame_num_offset: i32,
        nal_unit_type: NalUnitType,
        nal_ref_idc: u8,
    ) -> i32 {
        if nal_unit_type == NalUnitType::SliceIdr {
            return 0;
        }
        let abs = frame_num_offset.wrapping_add(frame_num as i32);
        if nal_ref_idc > 0 {
            abs.wrapping_mul(2)
        } else {
            abs.wrapping_mul(2) - 1
        }
    }

    /// Sliding window reference marking (spec 8.2.5.3).
    /// While short-term ref count >= max_ref_frames, mark the oldest as unused.
    fn sliding_window_mark(&mut self) {
        let max = self.max_ref_frames.max(1);
        while self.max_ref_frames > 0 {
            // Total ref count includes both short-term and long-term (spec 8.2.5.3).
            // For field pictures, two fields of a frame count as ONE toward
            // the limit: count distinct short-term frame_nums and long-term
            // frame indices (a long-term frame may share its frame_num with
            // a later short-term one once frame_num wraps).
            let mut seen_frames = BTreeSet::new();
            for e in &self.entries {
                match e.reference {
                    ReferenceStatus::ShortTerm => seen_frames.insert((false, e.pic.frame_num)),
                    ReferenceStatus::LongTerm(idx) => seen_frames.insert((true, idx)),
                    ReferenceStatus::Unused => false,
                };
            }
            let total_ref_count = seen_frames.len();
            if total_ref_count < max {
                break;
            }
            // Evict the oldest short-term reference (first in insertion order).
            // Using insertion order handles frame_num wraparound correctly
            // (spec 8.2.5.3: evict smallest FrameNumWrap, which is the oldest).
            if let Some(idx) = self
                .entries
                .iter()
                .position(|e| e.reference == ReferenceStatus::ShortTerm)
            {
                // Evict both fields of this frame_num (if any)
                let evict_fn = self.entries[idx].pic.frame_num;
                for e in &mut self.entries {
                    if e.reference == ReferenceStatus::ShortTerm && e.pic.frame_num == evict_fn {
                        e.reference = ReferenceStatus::Unused;
                    }
                }
            } else {
                break;
            }
        }
    }

    /// The frame_num of the frame a PicNum names (spec 8.2.4.1). PicNum counts back
    /// from the current frame_num, so it's negative for frames from before frame_num
    /// last wrapped, whose frame_num is it plus MaxFrameNum.
    pub fn frame_num_for_pic_num(pic_num: i32, max_frame_num: u32) -> u32 {
        pic_num.rem_euclid(max_frame_num.max(1) as i32) as u32
    }

    /// Whether `pic` is the picture an MMCO names: the frame `frame_num`,
    /// or with `field` set, that field of it (a frame entry stands for
    /// both its fields).
    fn names(pic: &DecodedPicture, frame_num: u32, field: Option<PictureStructure>) -> bool {
        pic.frame_num == frame_num
            && field.is_none_or(|f| pic.structure == f || pic.structure == PictureStructure::Frame)
    }

    /// The other field of the pair `pic` (a field) belongs to, if `other`
    /// is it: a long-term index shared with it is kept (spec 8.2.5.4.3).
    fn is_pair_field(pic: &DecodedPicture, other: &DecodedPicture) -> bool {
        pic.structure != PictureStructure::Frame
            && other.structure != PictureStructure::Frame
            && other.structure != pic.structure
            && other.frame_num == pic.frame_num
    }

    /// Mark a short-term reference as unused by frame_num, or one field of
    /// it with `field` set (MMCO op=1).
    pub fn mark_short_term_unused(&mut self, frame_num: u32, field: Option<PictureStructure>) {
        for entry in &mut self.entries {
            if entry.reference == ReferenceStatus::ShortTerm
                && Self::names(&entry.pic, frame_num, field)
            {
                // Both fields of a frame stored as a field pair
                entry.reference = ReferenceStatus::Unused;
            }
        }
        self.remove_unused();
    }

    /// Mark a long-term reference as unused by LongTermFrameIdx, or one
    /// field of it with `field` set (MMCO op=2).
    pub fn mark_long_term_unused(
        &mut self,
        long_term_frame_idx: u32,
        field: Option<PictureStructure>,
    ) {
        for entry in &mut self.entries {
            if entry.reference == ReferenceStatus::LongTerm(long_term_frame_idx)
                && field.is_none_or(|f| {
                    entry.pic.structure == f || entry.pic.structure == PictureStructure::Frame
                })
            {
                entry.reference = ReferenceStatus::Unused;
            }
        }
        self.remove_unused();
    }

    /// Assign a short-term reference to long-term with given index (MMCO op=3).
    /// First marks any existing long-term with the same index as unused.
    pub fn assign_long_term(
        &mut self,
        frame_num: u32,
        field: Option<PictureStructure>,
        long_term_frame_idx: u32,
    ) {
        // The named frame (both fields, when stored as a field pair) or field
        let targets: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                e.reference == ReferenceStatus::ShortTerm && Self::names(&e.pic, frame_num, field)
            })
            .collect();
        let Some(&first) = targets.first() else {
            return;
        };
        // Remove any other long-term with this index, except the other
        // field of the same frame
        let pic = Arc::clone(&self.entries[first].pic);
        for entry in &mut self.entries {
            if entry.reference == ReferenceStatus::LongTerm(long_term_frame_idx)
                && !Self::is_pair_field(&pic, &entry.pic)
            {
                entry.reference = ReferenceStatus::Unused;
            }
        }
        for i in targets {
            self.entries[i].reference = ReferenceStatus::LongTerm(long_term_frame_idx);
        }
        self.remove_unused();
    }

    /// Set max long-term frame index (MMCO op=4).
    /// All long-term refs with index > max are marked unused.
    /// max_long_term_frame_idx_plus1 = 0 means no long-term refs allowed.
    pub fn set_max_long_term_frame_idx(&mut self, max_long_term_frame_idx_plus1: u32) {
        if max_long_term_frame_idx_plus1 == 0 {
            // Mark ALL long-term refs as unused
            for entry in &mut self.entries {
                if matches!(entry.reference, ReferenceStatus::LongTerm(_)) {
                    entry.reference = ReferenceStatus::Unused;
                }
            }
        } else {
            let max_idx = max_long_term_frame_idx_plus1 - 1;
            for entry in &mut self.entries {
                if let ReferenceStatus::LongTerm(idx) = entry.reference {
                    if idx > max_idx {
                        entry.reference = ReferenceStatus::Unused;
                    }
                }
            }
        }
        self.remove_unused();
    }

    /// Clear all reference pictures (MMCO op=5).
    /// Marks all refs as unused and resets POC state.
    pub fn clear_all_refs(&mut self) {
        for entry in &mut self.entries {
            entry.reference = ReferenceStatus::Unused;
        }
        self.remove_unused();
        self.prev_poc_msb = 0;
        self.prev_poc_lsb = 0;
        // A picture with MMCO 5 counts as frame_num 0 and resets the offset
        self.prev_frame_num = 0;
        self.prev_frame_num_offset = 0;
    }

    /// Insert current picture as long-term reference with given index (MMCO op=6).
    /// Called after the picture is decoded and inserted into DPB.
    /// The picture must already be in the DPB as ShortTerm.
    pub fn mark_current_as_long_term(&mut self, long_term_frame_idx: u32, frame_num: u32) {
        // Remove any existing long-term with this index, except the first
        // field of the current picture's frame
        let current = self.entries.last().map(|e| Arc::clone(&e.pic));
        for entry in &mut self.entries {
            if entry.reference == ReferenceStatus::LongTerm(long_term_frame_idx)
                && !current
                    .as_ref()
                    .is_some_and(|c| Self::is_pair_field(c, &entry.pic))
            {
                entry.reference = ReferenceStatus::Unused;
            }
        }
        // Find the just-inserted picture and change to long-term
        for entry in self.entries.iter_mut().rev() {
            if entry.reference == ReferenceStatus::ShortTerm && entry.pic.frame_num == frame_num {
                entry.reference = ReferenceStatus::LongTerm(long_term_frame_idx);
                break;
            }
        }
        self.remove_unused();
    }

    /// Get list of long-term reference pictures, sorted by ascending long_term_frame_idx.
    /// Used to append to reference lists after short-term refs (spec 8.2.4.2.1, 8.2.4.2.3).
    pub fn long_term_ref_list(&self) -> Vec<Arc<DecodedPicture>> {
        let mut refs = self.long_term_refs(false);
        refs.sort_by_key(|(_, idx)| *idx);
        refs.into_iter().map(|(p, _)| p).collect()
    }

    /// Remove entries that are unused for reference (freeing memory).
    fn remove_unused(&mut self) {
        self.entries
            .retain(|e| e.reference != ReferenceStatus::Unused);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pic(frame_num: u32, poc: i32) -> Arc<DecodedPicture> {
        Arc::new(DecodedPicture::new_complete(
            PictureMeta {
                width: 16,
                height: 16,
                frame_num,
                pic_order_cnt: poc,
                field_poc: [poc, poc],
                mb_width: 1,
                is_intra: false,
                structure: crate::dpb::PictureStructure::Frame,
                chroma_422: false,
                sample_bytes: 1,
            },
            vec![],
            vec![],
            vec![],
            Motion::default(),
        ))
    }

    #[test]
    fn test_dpb_insert_and_clear() {
        let mut dpb = Dpb::new(4);
        dpb.insert(make_pic(0, 0), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(1, 2), ReferenceStatus::ShortTerm);
        assert_eq!(dpb.short_term_ref_list(15, 16, false, false).len(), 2);
        dpb.clear();
        assert_eq!(dpb.short_term_ref_list(15, 16, false, false).len(), 0);
    }

    #[test]
    fn test_sliding_window() {
        let mut dpb = Dpb::new(2);
        dpb.insert(make_pic(0, 0), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(1, 2), ReferenceStatus::ShortTerm);
        assert_eq!(dpb.short_term_ref_list(15, 16, false, false).len(), 2);

        // Third insert triggers sliding window — oldest (frame_num=0) gets evicted
        dpb.insert(make_pic(2, 4), ReferenceStatus::ShortTerm);
        let refs = dpb.short_term_ref_list(15, 16, false, false);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].frame_num, 2); // newest first (descending)
        assert_eq!(refs[1].frame_num, 1);
    }

    #[test]
    fn test_poc_type2() {
        assert_eq!(Dpb::compute_poc_type2(0, 0, NalUnitType::SliceIdr, 3), 0);
        assert_eq!(Dpb::compute_poc_type2(1, 0, NalUnitType::Slice, 3), 2);
        assert_eq!(Dpb::compute_poc_type2(1, 0, NalUnitType::Slice, 0), 1);
        assert_eq!(Dpb::compute_poc_type2(5, 0, NalUnitType::Slice, 1), 10);
        // After frame_num wraps (MaxFrameNum 256) the offset keeps POC rising
        assert_eq!(Dpb::compute_poc_type2(2, 256, NalUnitType::Slice, 1), 516);
    }

    #[test]
    fn test_ref_list_sorted_descending() {
        let mut dpb = Dpb::new(4);
        dpb.insert(make_pic(3, 6), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(1, 2), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(5, 10), ReferenceStatus::ShortTerm);

        let refs = dpb.short_term_ref_list(15, 16, false, false);
        assert_eq!(refs[0].frame_num, 5);
        assert_eq!(refs[1].frame_num, 3);
        assert_eq!(refs[2].frame_num, 1);
    }

    fn make_field(frame_num: u32, poc: i32, bottom: bool) -> Arc<DecodedPicture> {
        let pic = make_pic(frame_num, poc);
        let mut field = Arc::try_unwrap(pic).unwrap();
        field.structure = if bottom {
            PictureStructure::BottomField
        } else {
            PictureStructure::TopField
        };
        Arc::new(field)
    }

    #[test]
    fn test_ref_list_in_decoding_order_not_display_order() {
        // I0, then P4, then a reference B2 shown before it: the B was decoded
        // last, so it has the highest PicNum and leads the P-slice list
        // (spec 8.2.4.2.1), although P4 comes later in display order
        let mut dpb = Dpb::new(4);
        dpb.insert(make_pic(0, 0), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(1, 8), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(2, 4), ReferenceStatus::ShortTerm);
        let refs = dpb.short_term_ref_list(3, 16, false, false);
        let nums: Vec<u32> = refs.iter().map(|p| p.frame_num).collect();
        assert_eq!(nums, [2, 1, 0]);
    }

    #[test]
    fn test_ref_list_after_frame_num_wraps() {
        // FrameNumWrap counts frame_nums above the current one as before the
        // wrap (spec 8.2.4.1), so 0 is newer than 15 when the current is 1
        let mut dpb = Dpb::new(4);
        dpb.insert(make_pic(14, 28), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(15, 30), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(0, 32), ReferenceStatus::ShortTerm);
        let refs = dpb.short_term_ref_list(1, 16, false, false);
        let nums: Vec<u32> = refs.iter().map(|p| p.frame_num).collect();
        assert_eq!(nums, [0, 15, 14]);
    }

    #[test]
    fn test_field_ref_list_in_decoding_order_not_display_order() {
        // Field pairs I0, P1 and a reference B-pair 2 shown between them: the
        // frames go by FrameNumWrap too (spec 8.2.4.2.2), same parity first
        let mut dpb = Dpb::new(4);
        for (frame_num, poc) in [(0, 0), (1, 16), (2, 8)] {
            dpb.insert(
                make_field(frame_num, poc, false),
                ReferenceStatus::ShortTerm,
            );
            dpb.insert(
                make_field(frame_num, poc + 1, true),
                ReferenceStatus::ShortTerm,
            );
        }
        let refs = dpb.short_term_ref_list(3, 16, true, true);
        let order: Vec<(u32, bool)> = refs
            .iter()
            .map(|p| (p.frame_num, p.structure == PictureStructure::BottomField))
            .collect();
        assert_eq!(
            order,
            [
                (2, true),
                (2, false),
                (1, true),
                (1, false),
                (0, true),
                (0, false)
            ]
        );
    }

    #[test]
    fn test_mmco_after_frame_num_wraps_names_a_frame_from_before() {
        // x264's B-pyramid: the reference B-frame with frame_num 1 marks the one
        // two before it unused, PicNum -2, which is frame_num 14 (MaxFrameNum 16)
        let mut dpb = Dpb::new(5);
        for (frame_num, poc) in [(13, 48), (14, 44), (15, 50), (0, 58)] {
            dpb.insert(make_pic(frame_num, poc), ReferenceStatus::ShortTerm);
        }
        assert_eq!(Dpb::frame_num_for_pic_num(1 - 3, 16), 14);
        dpb.mark_short_term_unused(Dpb::frame_num_for_pic_num(1 - 3, 16), None);
        let mut left: Vec<u32> = dpb
            .short_term_refs(false)
            .iter()
            .map(|p| p.frame_num)
            .collect();
        left.sort_unstable();
        assert_eq!(left, [0, 13, 15]);
        assert_eq!(Dpb::frame_num_for_pic_num(3, 16), 3);
        assert_eq!(Dpb::frame_num_for_pic_num(-16, 16), 0);
    }

    fn long_term_set(dpb: &Dpb, is_field_pic: bool) -> Vec<(i32, u32)> {
        let mut v: Vec<(i32, u32)> = dpb
            .long_term_refs(is_field_pic)
            .iter()
            .map(|(p, idx)| (p.pic_order_cnt, *idx))
            .collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn test_field_mmco3_gives_both_fields_one_long_term_index() {
        // MR6_BT_B: an I field marks both fields of the previous frame
        // long-term with the same LongTermFrameIdx, one command per field
        let mut dpb = Dpb::new(5);
        dpb.insert(make_field(4, 8, false), ReferenceStatus::ShortTerm);
        dpb.insert(make_field(4, 9, true), ReferenceStatus::ShortTerm);
        dpb.assign_long_term(4, Some(PictureStructure::TopField), 0);
        dpb.assign_long_term(4, Some(PictureStructure::BottomField), 0);
        assert_eq!(long_term_set(&dpb, true), [(8, 0), (9, 0)]);
        assert!(dpb.short_term_refs(true).is_empty());
    }

    #[test]
    fn test_frame_mmco_names_both_fields_of_a_pair() {
        // MR7_BT_B: a frame picture marks a frame decoded as two fields
        let mut dpb = Dpb::new(5);
        dpb.insert(make_field(3, 6, false), ReferenceStatus::ShortTerm);
        dpb.insert(make_field(3, 7, true), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(4, 8), ReferenceStatus::ShortTerm);
        dpb.assign_long_term(3, None, 1);
        assert_eq!(long_term_set(&dpb, true), [(6, 1), (7, 1)]);
        dpb.mark_long_term_unused(1, None);
        assert!(dpb.long_term_refs(true).is_empty());
        dpb.mark_short_term_unused(4, None);
        assert!(dpb.short_term_refs(true).is_empty());
    }

    #[test]
    fn test_sliding_window_counts_long_term_frames_by_index() {
        // A long-term frame keeps its frame_num, which a short-term frame
        // reuses once frame_num wraps; they are still two frames
        let mut dpb = Dpb::new(3);
        dpb.insert(make_pic(6, 12), ReferenceStatus::LongTerm(0));
        dpb.insert(make_pic(5, 42), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(6, 44), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(7, 46), ReferenceStatus::ShortTerm);
        let mut left: Vec<u32> = dpb
            .short_term_refs(false)
            .iter()
            .map(|p| p.frame_num)
            .collect();
        left.sort_unstable();
        assert_eq!(left, [6, 7]);
        assert_eq!(long_term_set(&dpb, false), [(12, 0)]);
    }

    #[test]
    fn test_unused_not_in_ref_list() {
        let mut dpb = Dpb::new(4);
        dpb.insert(make_pic(0, 0), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(1, 2), ReferenceStatus::Unused); // non-reference
        assert_eq!(dpb.short_term_ref_list(15, 16, false, false).len(), 1);
    }

    #[test]
    fn test_ref_list_l0_b() {
        let mut dpb = Dpb::new(5);
        // POCs: 0, 2, 4, 6, 8 — current POC is 5
        dpb.insert(make_pic(0, 0), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(1, 2), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(2, 4), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(3, 6), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(4, 8), ReferenceStatus::ShortTerm);

        let l0 = dpb.ref_list_l0_b(5, false, false);
        // Before (POC <= 5): 4, 2, 0 (descending POC)
        // After (POC > 5): 6, 8 (ascending POC)
        assert_eq!(l0.len(), 5);
        assert_eq!(l0[0].pic_order_cnt, 4);
        assert_eq!(l0[1].pic_order_cnt, 2);
        assert_eq!(l0[2].pic_order_cnt, 0);
        assert_eq!(l0[3].pic_order_cnt, 6);
        assert_eq!(l0[4].pic_order_cnt, 8);
    }

    #[test]
    fn test_ref_list_l1_b() {
        let mut dpb = Dpb::new(5);
        dpb.insert(make_pic(0, 0), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(1, 2), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(2, 4), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(3, 6), ReferenceStatus::ShortTerm);
        dpb.insert(make_pic(4, 8), ReferenceStatus::ShortTerm);

        let l1 = dpb.ref_list_l1_b(5, false, false);
        // After (POC > 5): 6, 8 (ascending POC)
        // Before (POC <= 5): 4, 2, 0 (descending POC)
        assert_eq!(l1.len(), 5);
        assert_eq!(l1[0].pic_order_cnt, 6);
        assert_eq!(l1[1].pic_order_cnt, 8);
        assert_eq!(l1[2].pic_order_cnt, 4);
        assert_eq!(l1[3].pic_order_cnt, 2);
        assert_eq!(l1[4].pic_order_cnt, 0);
    }

    /// Regression test for a fuzz-discovered panic.
    ///
    /// `apply_ref_list_modification` panicked with "index out of bounds"
    /// when a malformed bitstream contained more RPLM commands than there
    /// were active reference slots. `ref_idx_lx` grew past `num_active`,
    /// causing an out-of-bounds index after the shift-right + push.
    ///
    /// Fix: bail out of the loop when `ref_idx_lx >= num_active`.
    #[test]
    fn test_fuzz_regression_rplm_too_many_ops_no_panic() {
        // Build a ref list with 2 entries
        let mut ref_list = vec![make_pic(0, 0), make_pic(1, 2)];
        // Feed 10 RPLM ops (far more than the 2 active slots)
        // idc=0 means "subtract abs_diff_pic_num from pred_pic_num"
        // With val=0 → abs_diff=1 → pic_num = pred - 1
        let ops: Vec<(u32, u32)> = (0..10).map(|_| (0u32, 0u32)).collect();
        // This must not panic
        let short_term = ref_list.clone();
        Dpb::apply_ref_list_modification(
            &mut ref_list,
            &ops,
            5,
            16,
            false,
            false,
            &short_term,
            &[],
        );
    }

    #[test]
    fn test_rplm_finds_a_reference_pushed_out_of_the_list() {
        // x264's weightp puts the same reference at indices 0 and 1 (with
        // different weights). With four references and four active entries,
        // the duplicate pushes the last of the initial list out, and the
        // final command then names it: it has to come from the DPB, not the
        // list. frame_num has wrapped at 16; the current picture's is 2.
        let short_term = [
            make_pic(1, 62),
            make_pic(0, 66),
            make_pic(15, 54),
            make_pic(14, 58),
        ];
        // The initial list, by descending FrameNumWrap: 1, 0, 15, 14
        let mut ref_list = short_term.to_vec();
        let ops = [(0, 1), (0, 15), (1, 0), (0, 2)];
        Dpb::apply_ref_list_modification(
            &mut ref_list,
            &ops,
            2,
            16,
            false,
            false,
            &short_term,
            &[],
        );
        let nums: Vec<u32> = ref_list.iter().map(|p| p.frame_num).collect();
        assert_eq!(nums, [0, 0, 1, 14]);
    }
}
