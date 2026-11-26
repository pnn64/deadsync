//! Frame-level multithreaded decoding with row-level reference
//! synchronization.
//!
//! The macroblock loop is inherently serial within a picture (CABAC and
//! intra/MV prediction depend on already-decoded neighbours), so parallelism
//! comes from decoding several *pictures* at once. [`ThreadedDecoder`] runs
//! all header-level work in coded order on the calling thread: parameter
//! sets, slice headers, POC and reference lists via `prepare_slice`, and
//! reference marking on a DPB of pictures that may still be decoding. Each
//! complete picture is handed to a worker thread.
//!
//! A worker deblocks and publishes its picture one macroblock row at a time
//! (see `RowSync`). Pictures that reference it wait only for the rows they
//! actually read: motion compensation waits for the reference rows its
//! interpolation filter touches, and direct-mode prediction waits for the
//! co-located picture to finish. A P-picture can therefore start while its
//! reference is still being decoded, so even a pure P chain is pipelined.
//!
//! The DPB bookkeeping is exactly the serial [`Decoder`]'s, performed in the
//! same order with the same metadata, and every sample a worker reads is
//! final when read, so the output is bit-identical to the serial decoder.
//! The reorder buffer reproduces [`OrderedDecoder`] semantics, so the
//! display-order frame sequence is the same as well; the tests decode every
//! test stream both ways and compare byte for byte.
//!
//! On streams with macroblock-level decode errors the threaded decoder
//! conceals rather than drops: a picture whose slices all fail becomes a
//! mid-grey stand-in, and rows published before a slice failed are kept.
//! Output can then differ from the serial decoder.
//!
//! Field pictures and MBAFF are supported.
//!
//! [`Decoder`]: crate::decoder::Decoder
//! [`OrderedDecoder`]: crate::decoder::OrderedDecoder

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::bitstream::BitstreamReader;
use crate::decoder::{
    apply_reference_marking, decoded_numbering, finish_output, has_mmco5, is_redundant_slice,
    prepare_slice, run_slice, store_parameter_set, Frame, PictureScratch, PictureState, RowSync,
    SlicePlan,
};
use crate::dpb::{DecodedPicture, Dpb, PictureMeta, PictureStructure, PictureWriter};
use crate::error::DecodeError;
use crate::nal::{NalUnit, NalUnitType};
use crate::pps::Pps;
use crate::slice::SliceType;
use crate::sps::Sps;

type WorkerResult = Result<PictureState, DecodeError>;

/// Picture currently accumulating slices on the coordinator.
struct OpenPicture {
    seq: u64,
    slices: Vec<SlicePlan>,
}

/// H.264 decoder that decodes several pictures concurrently and emits
/// frames in display order: the threaded counterpart of
/// [`OrderedDecoder`](crate::decoder::OrderedDecoder), with byte-identical
/// output.
///
/// ```no_run
/// use deadlib_video_h264::threading::ThreadedDecoder;
/// use deadlib_video_h264::nal::parse_annex_b;
///
/// let h264 = std::fs::read("input.h264").unwrap();
/// let nals = parse_annex_b(&h264);
/// let mut decoder = ThreadedDecoder::new(4);
/// for nal in &nals {
///     for frame in decoder.decode_nal(nal).unwrap() {
///         println!("POC {}", frame.pic_order_cnt);
///     }
/// }
/// for frame in decoder.flush() {
///     println!("POC {}", frame.pic_order_cnt);
/// }
/// ```
pub struct ThreadedDecoder {
    threads: usize,
    sps_table: BTreeMap<u32, Arc<Sps>>,
    pps_table: BTreeMap<u32, Arc<Pps>>,
    /// DPB of pictures that may still be decoding on workers.
    dpb: Dpb,
    open: Option<OpenPicture>,
    next_seq: u64,
    /// Next picture (in coded order) to hand to the output stage.
    commit_next: u64,
    /// Reorder-buffer GOP tag per dispatched picture.
    gops: HashMap<u64, u32>,
    results: BTreeMap<u64, WorkerResult>,
    done_tx: Sender<(u64, WorkerResult)>,
    done_rx: Receiver<(u64, WorkerResult)>,
    inflight: VecDeque<JoinHandle<()>>,
    /// First field of a field pair awaiting its complement (serial
    /// `Decoder::pending_field` equivalent; handled at commit time).
    pending_field: Option<Frame>,
    /// Whether the most recent commit produced an output frame.
    last_commit_output: bool,
    first_error: Option<DecodeError>,
    // Display-order reorder buffer with OrderedDecoder semantics.
    buffer: Vec<(u32, Frame)>,
    gop_id: u32,
    max_depth: usize,
}

impl ThreadedDecoder {
    /// Create a decoder that keeps up to `threads` pictures decoding
    /// concurrently (clamped to at least 1), with the default reorder depth.
    pub fn new(threads: usize) -> Self {
        Self::with_max_depth(threads, 16)
    }

    /// Like [`new`](Self::new) with an explicit reorder-buffer depth
    /// (see [`OrderedDecoder::with_max_depth`](crate::decoder::OrderedDecoder::with_max_depth)).
    pub fn with_max_depth(threads: usize, max_depth: usize) -> Self {
        let (done_tx, done_rx) = channel();
        Self {
            threads: threads.max(1),
            sps_table: BTreeMap::new(),
            pps_table: BTreeMap::new(),
            dpb: Dpb::new(0),
            open: None,
            next_seq: 0,
            commit_next: 0,
            gops: HashMap::new(),
            results: BTreeMap::new(),
            done_tx,
            done_rx,
            inflight: VecDeque::new(),
            pending_field: None,
            last_commit_output: false,
            first_error: None,
            buffer: Vec::new(),
            gop_id: 0,
            max_depth: max_depth.max(1),
        }
    }

    /// Number of pictures this decoder may decode concurrently.
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Frame rate from the most recently parsed SPS's VUI timing info, as
    /// `(numerator, denominator)`. See [`Decoder::frame_rate`](crate::decoder::Decoder::frame_rate).
    pub fn frame_rate(&self) -> Option<(u32, u32)> {
        self.sps_table.values().find_map(|s| s.frame_rate())
    }

    /// Frame rate as a single floating-point value.
    pub fn frame_rate_f64(&self) -> Option<f64> {
        let (n, d) = self.frame_rate()?;
        Some(n as f64 / d as f64)
    }

    /// Feed one NAL unit and return any frames that have become ready, in
    /// display order.
    ///
    /// Errors from slice headers are reported immediately; errors from
    /// macroblock decoding on a worker are reported on a later call.
    pub fn decode_nal(&mut self, nal: &NalUnit) -> Result<Vec<Frame>, DecodeError> {
        let mut out = Vec::new();
        match nal.nal_unit_type {
            NalUnitType::Sps | NalUnitType::Pps => {
                store_parameter_set(&mut self.sps_table, &mut self.pps_table, &mut self.dpb, nal)?;
            }
            NalUnitType::SliceIdr | NalUnitType::Slice => {
                if is_redundant_slice(&self.sps_table, &self.pps_table, nal) {
                    return Ok(out);
                }
                let mut peek = BitstreamReader::new(&nal.rbsp);
                let first_mb = peek.read_ue().unwrap_or(0);
                let is_new_picture = first_mb == 0;

                if is_new_picture {
                    // Same point at which the serial decoder finalizes the
                    // previous picture.
                    self.close_open_picture();
                    self.pump(&mut out);
                    while self.inflight.len() >= self.threads {
                        self.wait_one_completion();
                        self.pump(&mut out);
                    }
                }

                match prepare_slice(&self.sps_table, &self.pps_table, &mut self.dpb, nal) {
                    Ok(plan) => {
                        if self.open.is_none() {
                            let seq = self.next_seq;
                            self.next_seq += 1;
                            self.open = Some(OpenPicture {
                                seq,
                                slices: Vec::new(),
                            });
                        }
                        self.open.as_mut().unwrap().slices.push(plan);
                    }
                    // The serial decoder swallows a failed continuation slice
                    // when it already holds part of the picture.
                    Err(_) if self.open.is_some() => {}
                    Err(e) => return Err(e),
                }

                // OrderedDecoder bumps the GOP on every IDR NAL after pushing
                // the previous picture's frame; the open picture closed above
                // was tagged with the pre-bump value.
                if nal.nal_unit_type == NalUnitType::SliceIdr {
                    self.gop_id += 1;
                }
            }
            _ => {}
        }
        self.pump(&mut out);
        if let Some(e) = self.first_error.take() {
            return Err(e);
        }
        Ok(out)
    }

    /// Finish all in-flight pictures and return every remaining frame in
    /// display order. Call once after the last `decode_nal`.
    pub fn flush(&mut self) -> Vec<Frame> {
        let closed = self.close_open_picture();
        let mut out = Vec::new();
        while let Some(handle) = self.inflight.pop_front() {
            let _ = handle.join();
            self.pump(&mut out);
        }
        self.pump(&mut out);
        self.first_error = None;
        // Serial flush: finalize the last picture; if that produced no frame,
        // output a lone pending field as a half-height frame.
        if !closed || !self.last_commit_output {
            if let Some(field) = self.pending_field.take() {
                self.buffer.push((self.gop_id, field));
            }
        }
        self.buffer.sort_by_key(|(g, f)| (*g, f.pic_order_cnt));
        out.extend(self.buffer.drain(..).map(|(_, f)| f));
        out
    }

    /// Dispatch the open picture (if any) to a worker. Returns whether a
    /// picture was dispatched.
    fn close_open_picture(&mut self) -> bool {
        let Some(open) = self.open.take() else {
            return false;
        };
        let (Some(first), Some(last)) = (open.slices.first(), open.slices.last()) else {
            return false;
        };

        // The DPB picture is created now, empty, and filled by the worker.
        // Geometry comes from the slice that creates the PictureState, the
        // rest from the last slice, as in the serial decoder's finalize.
        let (frame_num, pic_order_cnt, field_poc) = decoded_numbering(
            &last.header.mmco_ops,
            last.header.frame_num,
            last.current_poc,
            last.field_poc,
        );
        let meta = PictureMeta {
            width: first.coded_width,
            height: first.coded_height,
            frame_num,
            pic_order_cnt,
            field_poc,
            mb_width: first.mb_width,
            is_intra: last.header.slice_type == SliceType::I,
            structure: if last.header.field_pic_flag {
                if last.header.bottom_field_flag {
                    PictureStructure::BottomField
                } else {
                    PictureStructure::TopField
                }
            } else {
                PictureStructure::Frame
            },
            chroma_422: first.sps.chroma_format_idc == 2,
            sample_bytes: if first.sps.bit_depth_luma_minus8 > 0 {
                2
            } else {
                1
            },
        };
        let (pic, writer) = DecodedPicture::new_in_progress(meta);
        apply_reference_marking(
            &mut self.dpb,
            pic,
            last.nal_unit_type,
            last.nal_ref_idc,
            last.header.frame_num,
            1u32 << (last.sps.log2_max_frame_num_minus4 + 4).min(31),
            last.header.field_pic_flag,
            &last.header.mmco_ops,
            last.header.adaptive_ref_pic_marking,
            last.header.long_term_reference_flag,
        );
        // A picture with MMCO 5 starts a new output period, as in
        // OrderedDecoder: it and later pictures are output after all earlier
        if has_mmco5(&last.header.mmco_ops) {
            self.gop_id += 1;
        }
        self.gops.insert(open.seq, self.gop_id);

        let seq = open.seq;
        let slices = open.slices;
        let done_tx = self.done_tx.clone();
        let handle = std::thread::spawn(move || {
            // A panic drops the writer during unwinding, which completes the
            // picture with stand-in content so no reader waits forever.
            let result = catch_unwind(AssertUnwindSafe(move || run_picture(slices, writer)))
                .unwrap_or(Err(DecodeError::InvalidSyntax("decode worker panicked")));
            let _ = done_tx.send((seq, result));
        });
        self.inflight.push_back(handle);
        true
    }

    /// Collect finished pictures and hand them to the output stage in coded
    /// order, emitting display-ready frames into `out`.
    fn pump(&mut self, out: &mut Vec<Frame>) {
        while let Ok((seq, res)) = self.done_rx.try_recv() {
            self.results.insert(seq, res);
        }
        self.inflight.retain(|h| !h.is_finished());

        while let Some(res) = self.results.remove(&self.commit_next) {
            let gop = self.gops.remove(&self.commit_next).unwrap_or(self.gop_id);
            self.commit_next += 1;
            match res {
                Ok(ps) => match finish_output(ps, &mut self.pending_field) {
                    Some(frame) => {
                        self.last_commit_output = true;
                        self.push_frame(gop, frame, out);
                    }
                    None => self.last_commit_output = false,
                },
                Err(e) => {
                    if self.first_error.is_none() {
                        self.first_error = Some(e);
                    }
                    self.last_commit_output = false;
                }
            }
        }
    }

    /// Block until a worker result arrives or a short timeout passes. The
    /// timeout matters because a result may already have been drained by an
    /// earlier `pump` while its handle still reads as running.
    fn wait_one_completion(&mut self) {
        match self.done_rx.recv_timeout(Duration::from_millis(20)) {
            Ok((seq, res)) => {
                self.results.insert(seq, res);
            }
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
    }

    /// Reorder-buffer push with `OrderedDecoder` semantics. Commits arrive in
    /// coded order, so the first frame of a newer GOP proves every older GOP
    /// is complete: drain them as one POC-sorted batch (what OrderedDecoder
    /// does when the IDR NAL arrives), then push and bound the depth.
    fn push_frame(&mut self, gop: u32, frame: Frame, out: &mut Vec<Frame>) {
        if self.buffer.iter().any(|(g, _)| *g < gop) {
            let mut completed: Vec<(u32, Frame)> = Vec::new();
            let mut remaining: Vec<(u32, Frame)> = Vec::with_capacity(self.buffer.len());
            for entry in self.buffer.drain(..) {
                if entry.0 < gop {
                    completed.push(entry);
                } else {
                    remaining.push(entry);
                }
            }
            completed.sort_by_key(|(g, f)| (*g, f.pic_order_cnt));
            out.extend(completed.into_iter().map(|(_, f)| f));
            self.buffer = remaining;
        }
        self.buffer.push((gop, frame));
        while self.buffer.len() > self.max_depth {
            let idx = self
                .buffer
                .iter()
                .enumerate()
                .min_by_key(|(_, (g, f))| (*g, f.pic_order_cnt))
                .map(|(i, _)| i)
                .unwrap();
            out.push(self.buffer.remove(idx).1);
        }
    }
}

impl Drop for ThreadedDecoder {
    fn drop(&mut self) {
        // Workers only wait on each other, never on the coordinator, so they
        // all finish on their own.
        while let Some(handle) = self.inflight.pop_front() {
            let _ = handle.join();
        }
    }
}

/// First macroblock index a slice writes (MB-pair addresses with MBAFF).
fn first_mb_index(plan: &SlicePlan) -> usize {
    let first = plan.header.first_mb_in_slice as usize;
    if plan.header.mbaff_frame_flag {
        first * 2
    } else {
        first
    }
}

/// Worker side: decode every slice of one picture, deblocking and publishing
/// rows as they become final, then complete the shared picture.
fn run_picture(slices: Vec<SlicePlan>, writer: PictureWriter) -> WorkerResult {
    let starts: Vec<usize> = slices.iter().map(first_mb_index).collect();
    let mut sync = RowSync {
        writer,
        next_unit: 0,
        limit_mb: 0,
    };
    let mut pending: Option<PictureState> = None;
    let mut scratch = PictureScratch::default();
    let mut recycled_frame = None;
    let mut first_error: Option<DecodeError> = None;
    for (k, plan) in slices.iter().enumerate() {
        // Rows a later slice may still write must not be deblocked or
        // published yet.
        sync.limit_mb = starts[k + 1..].iter().copied().min().unwrap_or(usize::MAX);
        // Mirror the serial decoder's recovery: a failed slice leaves the
        // picture as it was before that slice.
        let backup = pending.clone();
        let saved_unit = sync.next_unit;
        match run_slice(
            plan,
            pending.take(),
            &mut scratch,
            &mut recycled_frame,
            Some(&mut sync),
        ) {
            Ok(ps) => pending = Some(ps),
            Err(e) => {
                pending = backup;
                sync.next_unit = saved_unit;
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    // With no decoded state, dropping `sync` completes the picture as a
    // stand-in.
    let mut ps = match pending {
        Some(ps) => ps,
        None => return Err(first_error.unwrap_or(DecodeError::InvalidSyntax("empty picture"))),
    };
    sync.finish(&mut ps);
    Ok(ps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Readers block until the rows they need are published, see only the
    /// published prefix, and a dropped writer completes the picture.
    #[test]
    fn in_progress_picture_row_publication() {
        let meta = PictureMeta {
            width: 32,
            height: 48,
            frame_num: 0,
            pic_order_cnt: 0,
            field_poc: [0, 0],
            mb_width: 2,
            is_intra: false,
            structure: PictureStructure::Frame,
            chroma_422: false,
            sample_bytes: 1,
        };
        let (pic, mut writer) = DecodedPicture::new_in_progress(meta);
        assert!(pic.luma().is_empty());
        let y = vec![7u8; 32 * 48];
        let c = vec![9u8; 16 * 24];
        let reader = {
            let pic = Arc::clone(&pic);
            std::thread::spawn(move || {
                // Needs luma line 17, i.e. MB row 1.
                pic.wait_luma(32 * 18);
                pic.luma().len()
            })
        };
        std::thread::sleep(Duration::from_millis(20));
        writer.publish(&y, &c, &c, 1);
        assert_eq!(pic.luma().len(), 32 * 16);
        assert_eq!(pic.chroma(0).len(), 16 * 8);
        writer.publish(&y, &c, &c, 2);
        assert!(reader.join().unwrap() >= 32 * 32);
        // The last row is only published by finish/drop.
        writer.publish(&y, &c, &c, 3);
        assert_eq!(pic.luma().len(), 32 * 32);
        drop(writer);
        assert_eq!(pic.luma().len(), 32 * 48);
        assert!(pic.luma()[..32 * 32].iter().all(|&b| b == 7));
        assert!(pic.luma()[32 * 32..].iter().all(|&b| b == 128));
        assert_eq!(pic.motion().ref_idx_l0.len(), 2 * 3 * 16);
    }
}
