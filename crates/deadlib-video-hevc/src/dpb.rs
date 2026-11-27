//! Decoded picture buffer (DPB), reference picture sets and reference
//! picture lists (spec 8.3.2 - 8.3.4).
//!
//! HEVC references pictures by POC (picture order count) rather than by
//! index. The DPB holds recently decoded pictures, each marked as
//! "short-term reference", "long-term reference", or "unused for reference".
//! Pictures are removed once they are unused for reference; output order is
//! handled separately by `crate::output`.
//!
//! Pictures are shared as `Arc<DecodedPicture>` (with atomic marking
//! state) so the same picture can be held by the DPB and by a slice's
//! reference lists at once, without indices that compaction would
//! invalidate, and so worker threads can read reference pictures.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use crate::shared::SharedTable;

use crate::pixel::PixelData;

/// How this picture is currently marked in the DPB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PictureReferenceStatus {
    /// No future slice can reference this picture.
    UnusedForReference,
    /// Short-term reference (POC delta relative to the current picture).
    ShortTerm,
    /// Long-term reference (identified by POC LSB).
    LongTerm,
}

/// One slice's reference picture lists as seen by temporal MV prediction:
/// the POC of every entry and whether it was a long-term reference when the
/// slice was decoded (spec 8.5.3.2.8 compares long-term status).
#[derive(Debug, Clone, Default)]
pub struct SliceRefLists {
    pub pocs: [Vec<i32>; 2],
    pub is_long_term: [Vec<bool>; 2],
}

/// A picture's luma, Cb and Cr planes, in its sample type.
#[derive(Debug)]
pub enum PicturePlanes {
    U8([SharedTable<u8>; 3]),
    U16([SharedTable<u16>; 3]),
}

impl PicturePlanes {
    pub fn from_pixel_data(y: PixelData, u: PixelData, v: PixelData) -> Self {
        match (y, u, v) {
            (PixelData::U8(y), PixelData::U8(u), PixelData::U8(v)) => {
                PicturePlanes::U8([y.into(), u.into(), v.into()])
            }
            (PixelData::U16(y), PixelData::U16(u), PixelData::U16(v)) => {
                PicturePlanes::U16([y.into(), u.into(), v.into()])
            }
            _ => panic!("planes of different sample types"),
        }
    }
}

/// How far a picture is decoded, for pictures that reference it while it
/// is still decoding (frame threading). `decoded_ctb_rows`: CTB rows whose
/// CTBs are decoded (their motion field and slice map are final).
/// `final_rows`: luma sample rows that are final, after the loop filters
/// (the matching chroma rows are final too). A finished picture has both
/// at their maximum.
#[derive(Debug)]
pub struct PictureProgress {
    decoded_ctb_rows: AtomicUsize,
    final_rows: AtomicUsize,
    lock: Mutex<()>,
    wake: Condvar,
}

impl PictureProgress {
    /// A finished picture.
    pub fn complete() -> Self {
        Self::with(usize::MAX, usize::MAX)
    }

    /// A picture that is about to be decoded.
    pub fn started() -> Self {
        Self::with(0, 0)
    }

    fn with(decoded: usize, final_rows: usize) -> Self {
        Self {
            decoded_ctb_rows: AtomicUsize::new(decoded),
            final_rows: AtomicUsize::new(final_rows),
            lock: Mutex::new(()),
            wake: Condvar::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.final_rows.load(Ordering::Acquire) == usize::MAX
            && self.decoded_ctb_rows.load(Ordering::Acquire) == usize::MAX
    }

    /// Publish progress (`Release`: what the decoding thread wrote before is
    /// visible to a thread that observes it), waking waiters.
    pub fn publish(&self, decoded_ctb_rows: usize, final_rows: usize) {
        self.decoded_ctb_rows
            .store(decoded_ctb_rows, Ordering::Release);
        self.final_rows.store(final_rows, Ordering::Release);
        drop(self.lock.lock().unwrap());
        self.wake.notify_all();
    }

    /// The picture is finished (also when it failed: whatever was decoded,
    /// and the grey fill of the rest, is what references see).
    pub fn set_complete(&self) {
        self.publish(usize::MAX, usize::MAX);
    }

    fn wait_for(&self, ready: impl Fn() -> bool) {
        if ready() {
            return;
        }
        let mut guard = self.lock.lock().unwrap();
        while !ready() {
            guard = self.wake.wait(guard).unwrap();
        }
    }

    /// Wait until at least `n` CTB rows are decoded.
    pub fn wait_decoded_ctb_rows(&self, n: usize) {
        self.wait_for(|| self.decoded_ctb_rows.load(Ordering::Acquire) >= n);
    }

    /// Wait until at least `n` luma rows are final.
    pub fn wait_final_rows(&self, n: usize) {
        self.wait_for(|| self.final_rows.load(Ordering::Acquire) >= n);
    }

    /// Wait until the picture is finished.
    pub fn wait_complete(&self) {
        self.wait_for(|| self.is_complete());
    }
}

/// One decoded picture's planes + metadata.
///
/// With frame threading a picture is in the DPB, and referenced, while it
/// is still being decoded: its planes, motion field and slice map are
/// `SharedTable`s the decoding thread writes through views, and readers
/// wait on `progress` before reading. A sequentially decoded picture is
/// complete when it is created.
#[derive(Debug)]
pub struct DecodedPicture {
    /// Luma, Cb and Cr planes in raster order, with strides `y_stride` /
    /// `uv_stride`.
    pub planes: PicturePlanes,
    pub progress: PictureProgress,
    pub width: u32,
    pub height: u32,
    /// Signed picture order count (negative for leading pictures that
    /// precede an IDR or CRA in output order).
    pub poc: i32,
    /// Reference marking state (a `PictureReferenceStatus`), changed over
    /// the picture's lifetime in the DPB through `mark`. Atomic rather than
    /// a `Cell` so pictures can be shared with worker threads; marking only
    /// happens on the decoding thread, so `Relaxed` ordering is enough.
    reference_status: AtomicU8,
    /// Whether the picture has been output yet (emitted as a `Frame`).
    pub output: AtomicBool,

    /// per min-PU motion field stored when inserting into the DPB.
    /// Used by temporal merge/AMVP candidate derivation (spec 8.5.3.2.3 / 8.5.3.2.7).
    pub tab_mvf: SharedTable<crate::cu_tree::MvField>,
    /// `log2_min_pu_size` of this picture (needed to index `tab_mvf`).
    pub log2_min_pu_size: u8,
    /// min-PU width = `width >> log2_min_pu_size`.
    pub min_pu_width: usize,
    /// `log2_ctb_size` of this picture (needed for CTB row check in temporal candidate).
    pub log2_ctb_size: u8,
    /// per-list reference POCs at the time this picture was decoded.
    /// Used to compute POC distances for temporal MV scaling.
    /// `ref_pic_list_pocs[0]` = L0 POCs, `ref_pic_list_pocs[1]` = L1 POCs.
    pub ref_pic_list_pocs: [Vec<i32>; 2],

    /// Row stride of `y` in samples. Planes keep the CTU-aligned stride of
    /// the `PictureState` they came from, which is wider than `width` when
    /// the picture width is not a multiple of the CTB size.
    pub y_stride: usize,
    /// Row stride of `u` / `v` in samples.
    pub uv_stride: usize,
    /// log2(SubWidthC): 1 for 4:2:0, 0 for 4:4:4.
    pub chroma_shift_w: u32,
    /// log2(SubHeightC): 1 for 4:2:0, 0 for 4:4:4.
    pub chroma_shift_h: u32,
    /// Reference POC lists of each independent slice of this picture, keyed
    /// by slice address. Temporal MV prediction needs the lists of the slice
    /// that contains the collocated block (spec 8.5.3.2.9), which differ
    /// between slices when they use different reference list setups.
    /// Set once the picture's last slice is known (with frame threading,
    /// when its thread has all its slices; before any later picture reads
    /// it).
    pub slice_ref_pic_list_pocs: OnceLock<Vec<(i32, SliceRefLists)>>,
    /// Slice address of every CTB in raster order (`tab_slice_addr_rs` of
    /// the decoded picture).
    pub ctb_slice_addr: SharedTable<i32>,
}

impl DecodedPicture {
    pub fn new(
        y: PixelData,
        u: PixelData,
        v: PixelData,
        width: u32,
        height: u32,
        poc: i32,
    ) -> Self {
        Self {
            planes: PicturePlanes::from_pixel_data(y, u, v),
            progress: PictureProgress::complete(),
            width,
            height,
            poc,
            reference_status: AtomicU8::new(PictureReferenceStatus::ShortTerm as u8),
            output: AtomicBool::new(false),
            tab_mvf: SharedTable::default(),
            log2_min_pu_size: 2,
            min_pu_width: 0,
            log2_ctb_size: 0,
            ref_pic_list_pocs: [Vec::new(), Vec::new()],
            y_stride: width as usize,
            uv_stride: (width / 2) as usize,
            chroma_shift_w: 1,
            chroma_shift_h: 1,
            slice_ref_pic_list_pocs: OnceLock::new(),
            ctb_slice_addr: SharedTable::default(),
        }
    }

    /// Create a `DecodedPicture` with temporal MVP data (tab_mvf and ref lists)
    /// for use in temporal merge/AMVP candidate derivation.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_mvf(
        y: PixelData,
        u: PixelData,
        v: PixelData,
        width: u32,
        height: u32,
        poc: i32,
        tab_mvf: Vec<crate::cu_tree::MvField>,
        log2_min_pu_size: u8,
        min_pu_width: usize,
        log2_ctb_size: u8,
        ref_pic_list_pocs: [Vec<i32>; 2],
    ) -> Self {
        Self {
            planes: PicturePlanes::from_pixel_data(y, u, v),
            progress: PictureProgress::complete(),
            width,
            height,
            poc,
            reference_status: AtomicU8::new(PictureReferenceStatus::ShortTerm as u8),
            output: AtomicBool::new(false),
            tab_mvf: tab_mvf.into(),
            log2_min_pu_size,
            min_pu_width,
            log2_ctb_size,
            ref_pic_list_pocs,
            y_stride: width as usize,
            uv_stride: (width / 2) as usize,
            chroma_shift_w: 1,
            chroma_shift_h: 1,
            slice_ref_pic_list_pocs: OnceLock::new(),
            ctb_slice_addr: SharedTable::default(),
        }
    }

    /// A picture about to be decoded on its own thread (frame threading):
    /// it owns its planes, motion field and slice map, which the decoding
    /// thread writes through views (`PictureState::lend_shared`), and starts
    /// with no progress.
    #[allow(clippy::too_many_arguments)]
    pub fn in_progress(
        planes: PicturePlanes,
        tab_mvf: SharedTable<crate::cu_tree::MvField>,
        ctb_slice_addr: SharedTable<i32>,
        (width, height): (u32, u32),
        poc: i32,
        (log2_min_pu_size, min_pu_width, log2_ctb_size): (u8, usize, u8),
        (y_stride, uv_stride): (usize, usize),
        (chroma_shift_w, chroma_shift_h): (u32, u32),
    ) -> Self {
        Self {
            planes,
            progress: PictureProgress::started(),
            width,
            height,
            poc,
            reference_status: AtomicU8::new(PictureReferenceStatus::ShortTerm as u8),
            output: AtomicBool::new(false),
            tab_mvf,
            log2_min_pu_size,
            min_pu_width,
            log2_ctb_size,
            ref_pic_list_pocs: [Vec::new(), Vec::new()],
            y_stride,
            uv_stride,
            chroma_shift_w,
            chroma_shift_h,
            slice_ref_pic_list_pocs: OnceLock::new(),
            ctb_slice_addr,
        }
    }

    /// Reference lists in effect at luma position `(x, y)`: those of the
    /// slice containing that CTB, or `None` without per-slice information.
    pub fn ref_lists_at(&self, x: u32, y: u32) -> Option<&SliceRefLists> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let ctb = 1u32 << self.log2_ctb_size;
        let w_ctbs = self.width.div_ceil(ctb);
        let rs = ((y / ctb) * w_ctbs + x / ctb) as usize;
        let addr = self.ctb_slice_addr.get(rs)?;
        self.slice_ref_pic_list_pocs
            .get()?
            .iter()
            .find(|(a, _)| a == addr)
            .map(|(_, l)| l)
    }

    pub fn mark(&self, status: PictureReferenceStatus) {
        self.reference_status.store(status as u8, Ordering::Relaxed);
    }

    pub fn reference_status(&self) -> PictureReferenceStatus {
        match self.reference_status.load(Ordering::Relaxed) {
            0 => PictureReferenceStatus::UnusedForReference,
            1 => PictureReferenceStatus::ShortTerm,
            _ => PictureReferenceStatus::LongTerm,
        }
    }

    /// Whether the picture has been output (or is not to be output).
    pub fn is_output(&self) -> bool {
        self.output.load(Ordering::Relaxed)
    }

    pub fn set_output(&self) {
        self.output.store(true, Ordering::Relaxed);
    }

    pub fn is_reference(&self) -> bool {
        !matches!(
            self.reference_status(),
            PictureReferenceStatus::UnusedForReference
        )
    }
}

/// The decoded picture buffer: decoded pictures kept for reference (spec
/// 8.3.2 marking). Output order is handled separately by
/// `crate::output::OutputQueue`; a picture here is removed once it is no
/// longer used for reference.
#[derive(Debug, Default)]
pub struct DecodedPictureBuffer {
    pictures: Vec<Arc<DecodedPicture>>,
}

impl DecodedPictureBuffer {
    /// Create an empty DPB.
    pub fn new() -> Self {
        Self::default()
    }

    /// Discard all pictures (e.g. on a bit-depth change between sequences).
    pub fn clear(&mut self) {
        self.pictures.clear();
    }

    /// Insert a freshly decoded picture into the DPB. The new picture
    /// starts out marked as `ShortTerm` per HEVC conventions; callers
    /// that need a different initial state can `mark()` it afterwards.
    pub fn insert(&mut self, pic: Arc<DecodedPicture>) {
        self.pictures.push(pic);
    }

    /// Total number of pictures currently in the DPB.
    pub fn len(&self) -> usize {
        self.pictures.len()
    }

    /// Returns true when the DPB contains no pictures.
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
    }

    /// Find a picture by POC. Returns `Some(...)` for the first match,
    /// `None` if no picture in the DPB has the given POC.
    pub fn find_by_poc(&self, poc: i32) -> Option<Arc<DecodedPicture>> {
        self.pictures.iter().find(|p| p.poc == poc).cloned()
    }

    /// Find a short-term reference picture by POC. Like `find_by_poc`
    /// but skips pictures that aren't currently marked as ST references.
    pub fn find_st_ref(&self, poc: i32) -> Option<Arc<DecodedPicture>> {
        self.pictures
            .iter()
            .find(|p| p.poc == poc && p.reference_status() == PictureReferenceStatus::ShortTerm)
            .cloned()
    }

    /// Mark every reference picture in the DPB as `UnusedForReference`.
    /// Used when the current RPS has been fully resolved and we need to
    /// flip everything that wasn't referenced back to unused state.
    pub fn unmark_all_references(&self) {
        for p in &self.pictures {
            p.mark(PictureReferenceStatus::UnusedForReference);
        }
    }

    /// Remove every picture that is unused for reference and already
    /// output, keeping the order of the rest, and return the removed ones
    /// (so their buffers can be reused). Allocation-free: the vector is
    /// partitioned in place and the removed tail drained. Safe to call after
    /// every picture insertion.
    pub fn cleanup_unused(&mut self) -> std::vec::Drain<'_, Arc<DecodedPicture>> {
        let mut kept = 0;
        for i in 0..self.pictures.len() {
            let p = &self.pictures[i];
            if p.is_reference() || !p.is_output() {
                self.pictures.swap(kept, i);
                kept += 1;
            }
        }
        self.pictures.drain(kept..)
    }

    /// Direct access to the pictures list, for iteration by the caller
    /// when deriving the reference picture set.
    pub fn pictures(&self) -> &[Arc<DecodedPicture>] {
        &self.pictures
    }
}

/// The reference picture set of the current picture, in the five groups of
/// spec 8.3.2. The "Curr" sets feed `RefPicList0` / `RefPicList1`
/// construction; the "Foll" sets are kept for later pictures.
#[derive(Debug, Default, Clone)]
pub struct ReferencePictureSets {
    /// Short-term "before current picture" references that the current
    /// picture itself uses (used_by_curr_pic_s0_flag = 1).
    pub st_curr_before: Vec<i32>,
    /// Short-term "after current picture" references that the current
    /// picture itself uses.
    pub st_curr_after: Vec<i32>,
    /// Short-term references that the current picture does NOT use but
    /// which must still be kept around (they're referenced by later
    /// pictures according to the RPS).
    pub st_foll: Vec<i32>,
    /// Long-term references that the current picture uses. Each entry is
    /// a full POC (when `lt_poc_msb_present` is true for that index) or a
    /// POC LSB (when false).
    pub lt_curr: Vec<i32>,
    /// Long-term references that the current picture does not use but
    /// which later pictures will.
    pub lt_foll: Vec<i32>,
    /// Per-entry flag for `lt_curr` ++ `lt_foll` (in that order) indicating
    /// whether `delta_poc_msb_present_flag` was set. When true, the
    /// corresponding entry in `lt_curr`/`lt_foll` is a full POC and DPB
    /// matching uses exact POC. When false, it's a POC LSB and DPB matching
    /// uses `poc % MaxPicOrderCntLsb`.
    pub lt_poc_msb_present: Vec<bool>,
}

impl ReferencePictureSets {
    /// Total number of "current" references (= `NumPocTotalCurr` in the
    /// spec). Used to decide whether `ref_pic_lists_modification` can
    /// appear in the slice header, etc.
    pub fn num_poc_total_curr(&self) -> usize {
        self.st_curr_before.len() + self.st_curr_after.len() + self.lt_curr.len()
    }
}

/// construct `RefPicListTemp0` per spec 8.3.2.
///
/// The temp list is built by concatenating (in order) the POCs in
/// `st_curr_before`, `st_curr_after`, and `lt_curr`, repeating the
/// concatenation if necessary until the list is at least
/// `num_rps_curr_temp_list` entries long. Pure function on POCs so it
/// can be tested without a real DPB.
pub fn build_ref_pic_list_temp0(
    rps: &ReferencePictureSets,
    num_rps_curr_temp_list: usize,
) -> Vec<i32> {
    build_ref_pic_list_temp(
        &[&rps.st_curr_before, &rps.st_curr_after, &rps.lt_curr],
        num_rps_curr_temp_list,
    )
}

/// construct `RefPicListTemp1` per spec 8.3.2.
///
/// Same as `build_ref_pic_list_temp0` but with `st_curr_after` visited
/// **before** `st_curr_before` — for B-slice L1 construction.
pub fn build_ref_pic_list_temp1(
    rps: &ReferencePictureSets,
    num_rps_curr_temp_list: usize,
) -> Vec<i32> {
    build_ref_pic_list_temp(
        &[&rps.st_curr_after, &rps.st_curr_before, &rps.lt_curr],
        num_rps_curr_temp_list,
    )
}

/// Shared helper for L0 / L1 temp list construction. Takes a list of
/// sub-lists in visit order and concatenates them (wrapping around) until
/// the result has at least `target_len` entries.
fn build_ref_pic_list_temp(sub_lists: &[&Vec<i32>], target_len: usize) -> Vec<i32> {
    let mut out: Vec<i32> = Vec::with_capacity(target_len);
    if target_len == 0 {
        return out;
    }
    // Empty concatenation would loop forever; bail to avoid that (callers
    // should only ever build this list for P/B slices which by spec
    // require at least one reference).
    let total_in_sub_lists: usize = sub_lists.iter().map(|s| s.len()).sum();
    if total_in_sub_lists == 0 {
        return out;
    }
    while out.len() < target_len {
        for sub in sub_lists {
            for poc in sub.iter() {
                if out.len() >= target_len {
                    break;
                }
                out.push(*poc);
            }
            if out.len() >= target_len {
                break;
            }
        }
    }
    out
}

/// apply `ref_pic_list_modification()` to a temp list to
/// produce `RefPicList{0,1}` (spec 8.3.2 eq. 8-8 / 8-10). When the
/// modification flag is false, returns the first `num_active` entries of
/// the temp list unchanged. When true, the i-th output entry is
/// `temp_list[list_entry[i]]`.
pub fn apply_ref_pic_list_modification(
    temp_list: &[i32],
    modification_flag: bool,
    list_entry: &[u32],
    num_active: usize,
) -> Result<Vec<i32>, crate::error::DecodeError> {
    let mut out: Vec<i32> = Vec::with_capacity(num_active);
    if modification_flag {
        if list_entry.len() < num_active {
            return Err(crate::error::DecodeError::InvalidSyntax(
                "ref_pic_list_modification has fewer entries than active ref count",
            ));
        }
        for &idx in list_entry.iter().take(num_active) {
            let idx = idx as usize;
            if idx >= temp_list.len() {
                return Err(crate::error::DecodeError::InvalidSyntax(
                    "list_entry index out of temp list bounds",
                ));
            }
            out.push(temp_list[idx]);
        }
    } else {
        if temp_list.len() < num_active {
            return Err(crate::error::DecodeError::InvalidSyntax(
                "temp list shorter than active ref count",
            ));
        }
        out.extend_from_slice(&temp_list[..num_active]);
    }
    Ok(out)
}

/// Look up each reference-picture POC in the DPB: a short-term reference
/// first, then a long-term one, then any picture with that POC (marking may
/// not have settled for a damaged stream). An unresolved POC is an error.
///
/// `current` is the picture being decoded, which frame threading puts in
/// the DPB at its first slice: it is skipped, so the lists are those of a
/// sequential decode (where it enters the DPB once finished) even for a
/// damaged RPS naming the current POC. A picture listing itself would wait
/// forever for its own final rows.
pub fn resolve_ref_pics(
    dpb: &DecodedPictureBuffer,
    pocs: &[i32],
    current: Option<&Arc<DecodedPicture>>,
) -> Result<Vec<Arc<DecodedPicture>>, crate::error::DecodeError> {
    let candidates = |poc: i32| {
        dpb.pictures()
            .iter()
            .filter(move |p| p.poc == poc && !current.is_some_and(|c| Arc::ptr_eq(c, p)))
    };
    let mut out: Vec<Arc<DecodedPicture>> = Vec::with_capacity(pocs.len());
    for &poc in pocs {
        let pic = candidates(poc)
            .find(|p| p.reference_status() == PictureReferenceStatus::ShortTerm)
            .or_else(|| {
                candidates(poc).find(|p| p.reference_status() == PictureReferenceStatus::LongTerm)
            })
            .or_else(|| candidates(poc).next());
        match pic {
            Some(p) => out.push(Arc::clone(p)),
            None => {
                return Err(crate::error::DecodeError::InvalidSyntax(
                    "reference picture POC not present in DPB",
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_lookup_by_poc() {
        let mut dpb = DecodedPictureBuffer::new();
        let pic = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![0; 16 * 16]),
            PixelData::U8(vec![0; 8 * 8]),
            PixelData::U8(vec![0; 8 * 8]),
            16,
            16,
            5,
        ));
        dpb.insert(pic);
        assert_eq!(dpb.len(), 1);
        assert!(dpb.find_by_poc(5).is_some());
        assert!(dpb.find_by_poc(4).is_none());
    }

    #[test]
    fn mark_short_term_reference() {
        let pic = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            10,
        ));
        assert_eq!(pic.reference_status(), PictureReferenceStatus::ShortTerm);
        pic.mark(PictureReferenceStatus::LongTerm);
        assert_eq!(pic.reference_status(), PictureReferenceStatus::LongTerm);
        pic.mark(PictureReferenceStatus::UnusedForReference);
        assert!(!pic.is_reference());
    }

    /// Cleanup keeps the remaining pictures in insertion order and hands
    /// back exactly the removed ones.
    #[test]
    fn cleanup_keeps_order_and_returns_removed() {
        let pic = |poc: i32, referenced: bool, output: bool| {
            let p = Arc::new(DecodedPicture::new(
                PixelData::U8(vec![]),
                PixelData::U8(vec![]),
                PixelData::U8(vec![]),
                0,
                0,
                poc,
            ));
            if !referenced {
                p.mark(PictureReferenceStatus::UnusedForReference);
            }
            p.output.store(output, Ordering::Relaxed);
            p
        };
        let mut dpb = DecodedPictureBuffer::new();
        // Removable: unused for reference and output (POC 1 and 3).
        for (poc, r, o) in [
            (0, true, true),
            (1, false, true),
            (2, false, false),
            (3, false, true),
            (4, true, false),
        ] {
            dpb.insert(pic(poc, r, o));
        }
        let mut removed: Vec<i32> = dpb.cleanup_unused().map(|p| p.poc).collect();
        removed.sort();
        assert_eq!(removed, vec![1, 3]);
        let kept: Vec<i32> = dpb.pictures().iter().map(|p| p.poc).collect();
        assert_eq!(kept, vec![0, 2, 4]);
        assert_eq!(dpb.cleanup_unused().count(), 0);
    }

    #[test]
    fn cleanup_drops_unreferenced_and_output() {
        let mut dpb = DecodedPictureBuffer::new();
        let a = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            0,
        ));
        a.mark(PictureReferenceStatus::UnusedForReference);
        a.set_output();
        let b = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            1,
        ));
        // b is still a reference.
        dpb.insert(a);
        dpb.insert(b);
        dpb.cleanup_unused();
        assert_eq!(dpb.len(), 1);
        assert_eq!(dpb.pictures()[0].poc, 1);
    }

    #[test]
    fn num_poc_total_curr_sums_three_sets() {
        let rps = ReferencePictureSets {
            st_curr_before: vec![-1, -2],
            st_curr_after: vec![1],
            st_foll: vec![-4],
            lt_curr: vec![100],
            lt_foll: vec![],
            lt_poc_msb_present: vec![],
        };
        assert_eq!(rps.num_poc_total_curr(), 4);
    }

    #[test]
    fn ref_pic_list_temp0_order_before_then_after_then_lt() {
        // Spec 8.3.2: RefPicListTemp0 concatenates st_curr_before,
        // st_curr_after, lt_curr — in that order.
        let rps = ReferencePictureSets {
            st_curr_before: vec![3, 2],
            st_curr_after: vec![5, 6],
            st_foll: vec![],
            lt_curr: vec![100],
            lt_foll: vec![],
            lt_poc_msb_present: vec![],
        };
        // NumPocTotalCurr = 5, so with num_rps_curr_temp_list = 5 we expect
        // exactly the concatenation, no wrap.
        let temp = build_ref_pic_list_temp0(&rps, 5);
        assert_eq!(temp, vec![3, 2, 5, 6, 100]);
    }

    #[test]
    fn ref_pic_list_temp1_order_after_then_before_then_lt() {
        // Spec 8.3.2: RefPicListTemp1 concatenates st_curr_after,
        // st_curr_before, lt_curr — note After comes BEFORE Before.
        let rps = ReferencePictureSets {
            st_curr_before: vec![3, 2],
            st_curr_after: vec![5, 6],
            st_foll: vec![],
            lt_curr: vec![100],
            lt_foll: vec![],
            lt_poc_msb_present: vec![],
        };
        let temp = build_ref_pic_list_temp1(&rps, 5);
        assert_eq!(temp, vec![5, 6, 3, 2, 100]);
    }

    #[test]
    fn ref_pic_list_temp0_wraps_around_when_active_exceeds_rps() {
        // `num_ref_idx_l0_active > NumPocTotalCurr` → temp list wraps.
        // RPS = [3, 2], active = 5 → temp = [3, 2, 3, 2, 3].
        let rps = ReferencePictureSets {
            st_curr_before: vec![3, 2],
            st_curr_after: vec![],
            st_foll: vec![],
            lt_curr: vec![],
            lt_foll: vec![],
            lt_poc_msb_present: vec![],
        };
        let temp = build_ref_pic_list_temp0(&rps, 5);
        assert_eq!(temp, vec![3, 2, 3, 2, 3]);
    }

    #[test]
    fn ref_pic_list_modification_reorders_temp_list() {
        // list_entry_l0 = [1, 0] → result is [temp[1], temp[0]] = [42, 10].
        let temp = vec![10, 42, 99];
        let modified = apply_ref_pic_list_modification(&temp, true, &[1, 0], 2).expect("apply mod");
        assert_eq!(modified, vec![42, 10]);
    }

    #[test]
    fn ref_pic_list_modification_disabled_uses_temp_prefix() {
        let temp = vec![10, 42, 99, 7];
        let result = apply_ref_pic_list_modification(&temp, false, &[], 3).expect("no mod");
        assert_eq!(result, vec![10, 42, 99]);
    }

    #[test]
    fn ref_pic_list_modification_rejects_out_of_bounds_entry() {
        let temp = vec![10, 42];
        let err = apply_ref_pic_list_modification(&temp, true, &[5], 1);
        assert!(err.is_err());
    }

    #[test]
    fn resolve_ref_pics_looks_up_by_poc() {
        let mut dpb = DecodedPictureBuffer::new();
        let p0 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            2,
        ));
        let p1 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            5,
        ));
        dpb.insert(p0);
        dpb.insert(p1);
        let resolved = resolve_ref_pics(&dpb, &[5, 2], None).expect("resolve");
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].poc, 5);
        assert_eq!(resolved[1].poc, 2);
    }

    #[test]
    fn resolve_ref_pics_errors_on_missing_poc() {
        let dpb = DecodedPictureBuffer::new();
        let err = resolve_ref_pics(&dpb, &[3], None);
        assert!(err.is_err());
    }

    /// resolve_ref_pics prefers ST-marked pictures over unmarked ones.
    #[test]
    fn resolve_ref_pics_prefers_short_term() {
        let mut dpb = DecodedPictureBuffer::new();
        let p = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            7,
        ));
        // Default status is ShortTerm.
        assert_eq!(p.reference_status(), PictureReferenceStatus::ShortTerm);
        dpb.insert(p);
        let resolved = resolve_ref_pics(&dpb, &[7], None).expect("resolve");
        assert_eq!(resolved[0].poc, 7);
        assert_eq!(
            resolved[0].reference_status(),
            PictureReferenceStatus::ShortTerm
        );
    }

    /// resolve_ref_pics finds LT-marked pictures by POC.
    #[test]
    fn resolve_ref_pics_finds_long_term() {
        let mut dpb = DecodedPictureBuffer::new();
        let p = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            7,
        ));
        p.mark(PictureReferenceStatus::LongTerm);
        dpb.insert(p);
        let resolved = resolve_ref_pics(&dpb, &[7], None).expect("resolve");
        assert_eq!(
            resolved[0].reference_status(),
            PictureReferenceStatus::LongTerm
        );
    }

    /// resolve_ref_pics falls back to unmarked pictures (needed during
    /// the transition window before apply_rps_marking runs at picture
    /// completion).
    #[test]
    fn resolve_ref_pics_fallback_unmarked() {
        let mut dpb = DecodedPictureBuffer::new();
        let p = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            7,
        ));
        p.mark(PictureReferenceStatus::UnusedForReference);
        dpb.insert(p);
        let resolved = resolve_ref_pics(&dpb, &[7], None).expect("resolve");
        assert_eq!(resolved[0].poc, 7);
    }

    /// apply_rps_marking marks ST refs and leaves others unused.
    #[test]
    fn apply_rps_marking_marks_st_and_unused() {
        use crate::decoder::Decoder;

        let mut dpb = DecodedPictureBuffer::new();
        let p0 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            0,
        ));
        let p4 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            4,
        ));
        let p8 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            8,
        ));
        dpb.insert(p0.clone());
        dpb.insert(p4.clone());
        dpb.insert(p8.clone());

        // RPS: only POC 0 and 8 are ST refs. POC 4 is not referenced.
        let mut rps = ReferencePictureSets {
            st_curr_before: vec![0],
            st_curr_after: vec![8],
            st_foll: vec![],
            lt_curr: vec![],
            lt_foll: vec![],
            lt_poc_msb_present: vec![],
        };
        Decoder::apply_rps_marking(&mut rps, &dpb, 8);

        assert_eq!(p0.reference_status(), PictureReferenceStatus::ShortTerm);
        assert_eq!(
            p4.reference_status(),
            PictureReferenceStatus::UnusedForReference
        );
        assert_eq!(p8.reference_status(), PictureReferenceStatus::ShortTerm);
    }

    /// apply_rps_marking: LT marking uses POC LSB and doesn't override ST.
    #[test]
    fn apply_rps_marking_lt_by_lsb_no_st_override() {
        use crate::decoder::Decoder;

        let mut dpb = DecodedPictureBuffer::new();
        // Two pictures: POC 3 (ST ref) and POC 19 (LT ref by LSB=3 with max_lsb=16).
        let p3 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            3,
        ));
        let p19 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            19,
        ));
        dpb.insert(p3.clone());
        dpb.insert(p19.clone());

        let mut rps = ReferencePictureSets {
            st_curr_before: vec![3],
            st_curr_after: vec![],
            st_foll: vec![],
            lt_curr: vec![3], // LSB=3, matches both POC 3 and POC 19 (19%16=3)
            lt_foll: vec![],
            lt_poc_msb_present: vec![false, false], // LSB-based matching for both
        };
        // log2_max_poc_lsb=4 → max_poc_lsb=16
        Decoder::apply_rps_marking(&mut rps, &dpb, 4);

        // POC 3 should be ST (ST takes priority over LT).
        assert_eq!(p3.reference_status(), PictureReferenceStatus::ShortTerm);
        // POC 19 (LSB=3) should be LT (not overridden by ST since POC 19 != 3).
        assert_eq!(p19.reference_status(), PictureReferenceStatus::LongTerm);
    }

    /// LT marking with poc_msb_present=true uses full POC matching (not LSB).
    #[test]
    fn apply_rps_marking_lt_full_poc_matching() {
        use crate::decoder::Decoder;

        let mut dpb = DecodedPictureBuffer::new();
        // Two pictures with the SAME POC LSB (3) but different full POCs.
        // log2_max_poc_lsb=4 → max_poc_lsb=16
        let p3 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            3,
        ));
        let p19 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            19,
        ));
        dpb.insert(p3.clone());
        dpb.insert(p19.clone());

        // LT ref with poc_msb_present=true and full POC=19 → should match
        // only POC 19, not POC 3 (even though both have LSB=3).
        let mut rps = ReferencePictureSets {
            st_curr_before: vec![],
            st_curr_after: vec![],
            st_foll: vec![],
            lt_curr: vec![19], // full POC (msb_present=true)
            lt_foll: vec![],
            lt_poc_msb_present: vec![true],
        };
        Decoder::apply_rps_marking(&mut rps, &dpb, 4);

        assert_eq!(
            p3.reference_status(),
            PictureReferenceStatus::UnusedForReference
        );
        assert_eq!(p19.reference_status(), PictureReferenceStatus::LongTerm);
    }

    /// LT marking with poc_msb_present=false uses LSB matching.
    #[test]
    fn apply_rps_marking_lt_lsb_matching() {
        use crate::decoder::Decoder;

        let mut dpb = DecodedPictureBuffer::new();
        // POC 19 has LSB=3 (19%16=3)
        let p19 = Arc::new(DecodedPicture::new(
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            PixelData::U8(vec![]),
            0,
            0,
            19,
        ));
        dpb.insert(p19.clone());

        // LT ref with poc_msb_present=false and LSB=3 → matches POC 19
        let mut rps = ReferencePictureSets {
            st_curr_before: vec![],
            st_curr_after: vec![],
            st_foll: vec![],
            lt_curr: vec![3], // POC LSB (msb_present=false)
            lt_foll: vec![],
            lt_poc_msb_present: vec![false],
        };
        Decoder::apply_rps_marking(&mut rps, &dpb, 4);

        assert_eq!(p19.reference_status(), PictureReferenceStatus::LongTerm);
    }
}
