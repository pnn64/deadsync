//! Top-level HEVC decoder.
//!
//! The decoder retains parameter sets, assembles picture slices and drives
//! reconstruction, in-loop filtering, reference-picture management and output.

use std::sync::Arc;

use crate::cabac::{CabacContexts, CabacReader};
use crate::cu_tree::{MvField, PictureState, RecycledBuffers};
use crate::dpb::{
    DecodedPicture, DecodedPictureBuffer, PictureReferenceStatus, ReferencePictureSets,
    apply_ref_pic_list_modification, build_ref_pic_list_temp0, build_ref_pic_list_temp1,
    resolve_ref_pics,
};
use crate::error::DecodeError;
use crate::nal::{NalUnit, NalUnitType};
use crate::pixel::{Pixel, PixelData};
use crate::pps::{Pps, parse_pps};
use crate::shared::SharedTable;
use crate::slice::{SliceHeader, SliceType, parse_slice_segment_header};
use crate::sps::{ShortTermRps, Sps, parse_sps};

/// Reference picture lists L0 and L1 of one slice.
type RefPicLists = (Vec<Arc<DecodedPicture>>, Vec<Arc<DecodedPicture>>);
use crate::vps::{Vps, parse_vps};

/// A reconstructed video frame with cropped planar Y, Cb and Cr samples.
///
/// Plane lengths are `width * height` for luma and `(width/2) * (height/2)`
/// for each chroma plane. `pic_order_cnt` is the full (signed) POC
/// computed per spec 8.3.1 — 0 for IDR pictures, and the MSB-extended LSB
/// for non-IDR pictures.
///
/// Pixel data is stored in a `PixelData` enum that can be either 8-bit (`U8`)
/// or 10/12-bit (`U16`). Use `bit_depth` to distinguish; call
/// `y.as_u8()` / `y.as_u16()` to access the underlying slices.
///
/// Non-exhaustive: fields may be added in minor releases. Read fields
/// directly or destructure with `..`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Frame {
    /// Luma plane, `width * height` samples in raster order.
    pub y: PixelData,
    /// Cb plane. Half the luma width and height for 4:2:0, full size for
    /// 4:4:4 (see `chroma_format_idc`).
    pub u: PixelData,
    /// Cr plane, the same size as `u`.
    pub v: PixelData,
    /// Luma width in samples, after cropping to the conformance window.
    pub width: u32,
    /// Luma height in samples, after cropping to the conformance window.
    pub height: u32,
    /// Picture order count: the picture's position in display order within
    /// its coded video sequence. It restarts at every IDR or BLA picture.
    pub pic_order_cnt: i32,
    /// Bit depth of the luma samples: 8, 10, or 12. Chroma samples
    /// normally have the same depth; a stream may code them at a different
    /// one (BitDepthC), and they are stored at that depth.
    pub bit_depth: u8,
    /// `chroma_format_idc`: 1 = 4:2:0, 2 = 4:2:2, 3 = 4:4:4. Determines the
    /// dimensions of the `u`/`v` planes relative to `width`/`height`
    /// (`SubWidthC`/`SubHeightC`, spec table 6-1).
    pub chroma_format_idc: u32,
}

/// Runtime-dispatched picture state that can hold either 8-bit or 16-bit pixels.
// Pictures and picture state are shared with, or moved to, worker
// threads; a non-`Send` field (an `Rc`, a `RefCell`) would break that, and
// `Decoder: Send` is part of the public API.
const _: () = {
    const fn send<T: Send>() {}
    const fn send_sync<T: Send + Sync>() {}
    send::<Decoder>();
    send::<PictureStateEnum>();
    send_sync::<crate::dpb::DecodedPicture>();
};

enum PictureStateEnum {
    U8(PictureState<u8>),
    U16(PictureState<u16>),
}

/// Helper macro to dispatch on a `PictureStateEnum`, binding the inner
/// `PictureState<P>` to `$state` and executing `$body` in a context where
/// `P: Pixel` is known at compile time.
macro_rules! with_picture_state {
    ($pic:expr, |$state:ident| $body:expr) => {
        match $pic {
            &mut PictureStateEnum::U8(ref mut $state) => $body,
            &mut PictureStateEnum::U16(ref mut $state) => $body,
        }
    };
}

/// In-flight picture state: the reconstruction buffers plus the bookkeeping
/// needed to stitch multi-slice decode back together.
struct PictureInProgress {
    /// Where the picture decodes: on the calling thread, or on its own
    /// thread (frame threading).
    side: PictureSide,
    /// Header of the most recently decoded slice segment. Phase 3c-1 uses
    /// it for deblock/SAO finalization — in the common case all slices in a
    /// picture share the same filter flags, which this approximation
    /// matches.
    last_slice_header: SliceHeader,
    /// Phase 3c-4: header of the most recent *independent* slice segment
    /// in this picture. A dependent slice segment inherits `slice_type`,
    /// `slice_qp_y`, `slice_sao_*`, and the deblock filter fields from
    /// this header; the parser leaves those fields at default values on
    /// dependent slices and the decoder copies them in from here. For a
    /// picture that doesn't use dependent slice segments this is simply a
    /// copy of `last_slice_header` after every slice.
    last_independent_slice_header: SliceHeader,
    /// Total CTBs in the picture = `pic_width_in_ctbs * pic_height_in_ctbs`.
    total_ctbs: u32,
    /// Reference POC lists of each independent slice, keyed by its slice
    /// address (the value `tab_slice_addr_rs` holds for its CTBs). Stored in
    /// the decoded picture for temporal MV prediction by later pictures.
    slice_ref_pocs: Vec<(i32, crate::dpb::SliceRefLists)>,
    /// NAL unit type of the picture's first slice; every slice of a picture
    /// has the same one (spec 7.4.2.4.4).
    nal_unit_type: NalUnitType,
}

/// Tile scan derivation tables for a given (SPS, PPS) pair (spec 6.5.1).
///
/// `ctb_addr_rs_to_ts[rs]` = tile-scan address for a CTB at raster address
/// `rs`. `ctb_addr_ts_to_rs[ts]` is the inverse. `tile_id[rs]` gives the
/// 0-based tile index of the CTB at raster address `rs`.
///
/// Single-tile (`num_tile_columns = 1 && num_tile_rows = 1`) degenerates to
/// identity tables and `tile_id` = 0 everywhere, preserving raster scan.
/// The decode side of a picture in progress: its state and what its CTB
/// decode carries from one slice segment to the next. With frame threading
/// it lives on the picture's thread.
struct PictureDecode {
    state: PictureStateEnum,
    /// CABAC context state at the end of the most recently decoded slice
    /// segment, for a following dependent slice segment.
    saved_cabac_state: Option<CabacContexts>,
    /// WPP context state saved after the second CTB of the latest CTB row,
    /// for a dependent slice segment that starts a row.
    saved_wpp_cabac_state: Option<CabacContexts>,
    /// Frame threading: decode WPP slices' CTB rows in parallel even though
    /// the picture is filtered row by row (set for a picture that started
    /// with no other picture in flight, such as a still image).
    rows_in_parallel: bool,
}

impl PictureDecode {
    /// The end of the picture's decode: grey out CTBs no slice decoded,
    /// run the loop filters (with the flags of the picture's last slice
    /// header `last_sh`, as before) and return the cropped frame.
    fn finish(
        &mut self,
        sps: &Sps,
        pps: &Pps,
        last_sh: &SliceHeader,
        threads: usize,
    ) -> Result<Frame, DecodeError> {
        with_picture_state!(&mut self.state, |state| fill_undecoded_ctbs(state));
        with_picture_state!(&mut self.state, |state| {
            // Per-slice: CUs of slices with deblocking disabled have no
            // boundary strengths, so this only filters enabled slices.
            if let Some(filter) = state.row_filter.take() {
                // Frame threading: the rest of the rows (after the grey fill).
                filter.finish(state);
            } else {
                crate::deblock::deblock_picture(state, sps, pps, last_sh, threads);
                crate::sao::apply_sao_picture(state, sps, threads);
            }
            crop_frame(
                state,
                sps.pic_width_in_luma_samples,
                sps.pic_height_in_luma_samples,
                sps.cropped_width(),
                sps.cropped_height(),
                sps.conf_win_left_offset,
                sps.conf_win_top_offset,
                last_sh.poc,
                sps.bit_depth_luma,
            )
        })
    }
}

/// Where a picture in progress is decoded.
enum PictureSide {
    /// On the calling thread, slice by slice as `decode_nal` receives them.
    Local(Box<PictureDecode>),
    /// On its own thread (frame threading).
    Thread(Box<PictureThread>),
}

/// The calling thread's handle on a picture decoding on its own thread.
struct PictureThread {
    /// Identifies the picture's pending frame in the output queue.
    id: u64,
    jobs: std::sync::mpsc::Sender<PictureMessage>,
    handle: crate::parallel::TaskHandle<PictureResult>,
    picture: Arc<DecodedPicture>,
}

enum PictureMessage {
    Slice(Box<SliceJob>),
    /// No more slices: finish the picture (loop filters, crop) with these
    /// parameter sets and the picture's last slice header, as a sequential
    /// decode does.
    Finish {
        sps: Arc<Sps>,
        pps: Arc<Pps>,
        last_sh: Box<SliceHeader>,
    },
}

/// What a picture thread returns.
struct PictureResult {
    frame: Result<Frame, DecodeError>,
    /// Errors from the picture's slice data, in slice order.
    errors: Vec<DecodeError>,
    /// The picture state, for recycling (its lent tables released).
    state: PictureStateEnum,
}

/// A picture whose last slice has been handed to its thread.
struct InFlight {
    id: u64,
    handle: crate::parallel::TaskHandle<PictureResult>,
    /// PicOutputFlag: whether its frame is output.
    output: bool,
}

/// The body of a picture thread: decode the slices it receives (the loop
/// filters follow row by row, publishing final rows to the pictures that
/// reference this one), then finish the picture and mark it complete.
fn picture_thread(
    mut decode: PictureDecode,
    jobs: std::sync::mpsc::Receiver<PictureMessage>,
    picture: Arc<DecodedPicture>,
    threads: usize,
) -> PictureResult {
    // A picture that stops early (a panic, or the decoder dropped) must
    // still release the pictures waiting on it.
    struct CompleteOnDrop(Arc<DecodedPicture>);
    impl Drop for CompleteOnDrop {
        fn drop(&mut self) {
            self.0.progress.set_complete();
        }
    }
    let guard = CompleteOnDrop(picture);
    let mut errors = Vec::new();
    let mut finish = None;
    for message in jobs {
        match message {
            PictureMessage::Slice(job) => {
                // References are read as their rows become final (motion
                // compensation and temporal MV prediction wait for the rows
                // they need).
                if let Err(e) = decode.decode_slice(&job, threads) {
                    errors.push(e);
                }
            }
            PictureMessage::Finish { sps, pps, last_sh } => {
                finish = Some((sps, pps, last_sh));
                break;
            }
        }
    }
    let frame = match finish {
        Some((sps, pps, last_sh)) => decode.finish(&sps, &pps, &last_sh, threads),
        None => Err(DecodeError::InvalidSyntax(
            "picture abandoned before its end",
        )),
    };
    // The views of the picture's planes end before it is marked complete
    // (and others read it).
    with_picture_state!(&mut decode.state, |s| s.release_lent());
    drop(guard);
    PictureResult {
        frame,
        errors,
        state: decode.state,
    }
}

/// Start decoding a new picture on its own thread: the picture enters the
/// DPB now, owning its planes, motion field and slice map (the state keeps
/// views of them), so later pictures can reference it while it decodes.
#[allow(clippy::too_many_arguments)]
fn start_picture_thread(
    workers: &crate::parallel::ThreadPool,
    dpb: &mut DecodedPictureBuffer,
    pool: &mut PicturePool,
    id: u64,
    threads: usize,
    mut state: PictureStateEnum,
    poc: i32,
    (sps, pps): (&Arc<Sps>, &Arc<Pps>),
    rows_in_parallel: bool,
) -> PictureThread {
    fn in_progress<P: Pixel>(
        st: &mut PictureState<P>,
        poc: i32,
        sps: &Arc<Sps>,
        pps: &Arc<Pps>,
    ) -> Arc<DecodedPicture> {
        // SAFETY: the decoded picture owns the lent tables; the picture
        // thread holds an `Arc` of it until after `release_lent` has
        // dropped the state's views, and other pictures read it only once
        // it is complete (they wait on its progress first).
        let (planes, mvf, slice_addr) = unsafe { st.lend_shared() };
        let picture = Arc::new(DecodedPicture::in_progress(
            P::wrap_planes(planes),
            mvf,
            slice_addr,
            (st.width, st.height),
            poc,
            (st.log2_min_pu_size, st.min_pu_width, st.log2_ctb_size),
            (st.y_stride, st.uv_stride),
            (st.chroma_shift_w, st.chroma_shift_h),
        ));
        let filter = crate::rowfilter::RowFilter::new(
            st,
            Arc::clone(&picture),
            Arc::clone(sps),
            Arc::clone(pps),
        );
        st.row_filter = Some(Box::new(filter));
        picture
    }
    let picture = with_picture_state!(&mut state, |st| in_progress(st, poc, sps, pps));
    picture.mark(PictureReferenceStatus::ShortTerm);
    picture.set_output();
    dpb.insert(Arc::clone(&picture));
    for retired in dpb.cleanup_unused() {
        pool.recycle_picture(retired);
    }
    let (jobs, receiver) = std::sync::mpsc::channel();
    let decode = PictureDecode {
        state,
        saved_cabac_state: None,
        saved_wpp_cabac_state: None,
        rows_in_parallel,
    };
    let for_thread = Arc::clone(&picture);
    let handle = workers.spawn(move || picture_thread(decode, receiver, for_thread, threads));
    PictureThread {
        id,
        jobs,
        handle,
        picture,
    }
}

/// One slice segment, parsed and set up by the calling thread, with
/// everything its CTB decode needs, owned (frame threading runs the decode
/// on the picture's thread).
struct SliceJob {
    rbsp: Vec<u8>,
    epb_positions: Vec<u32>,
    sh: SliceHeader,
    sps: Arc<Sps>,
    pps: Arc<Pps>,
    tile_tables: Arc<TileScanTables>,
    slice_params: crate::cu_tree::SliceParams,
    /// The slice address CTBs record: that of the independent slice
    /// segment (the segment's own when it is independent).
    recorded_slice_addr: i32,
    slice_start_ts: u32,
    slice_starts_tile: bool,
    tile_col_start: Vec<u32>,
    /// The segment starts a QP prediction chain (spec 8.6.1).
    segment_starts_qp_chain: bool,
    /// RBSP byte offset of the slice data.
    cabac_byte_offset: usize,
    /// NAL-unit byte offset of the slice data (entry points count EPBs).
    nal_slice_data_start: u32,
}

impl PictureDecode {
    /// CTB decode of one slice segment (spec 7.3.8): CABAC set-up, then the
    /// CTB loop, sequential or wavefront.
    fn decode_slice(&mut self, job: &SliceJob, threads: usize) -> Result<(), DecodeError> {
        let (sps, pps, sh) = (&*job.sps, &*job.pps, &job.sh);
        let tile_tables = &*job.tile_tables;
        let pic_width_in_ctbs_for_init = sps.pic_width_in_ctbs_y();
        let wpp_for_init = pps.entropy_coding_sync_enabled_flag;
        let slice_starts_tile = job.slice_starts_tile;
        let tile_col_start = &job.tile_col_start;
        let slice_start_ts = job.slice_start_ts;
        let cabac_byte_offset = job.cabac_byte_offset;
        let nal_slice_data_start = job.nal_slice_data_start;
        let epb_positions: &[u32] = &job.epb_positions;
        let recorded_slice_addr = job.recorded_slice_addr;
        let pic_width_in_ctbs = sps.pic_width_in_ctbs_y();
        let total_ctbs = pic_width_in_ctbs * sps.pic_height_in_ctbs_y();

        if sh.dependent_slice_segment_flag {
            // The CTB before a dependent segment must belong to its slice:
            // otherwise the segment before it was lost or failed, and its
            // CABAC state and slice address are not this segment's.
            let prev_rs = slice_start_ts
                .checked_sub(1)
                .and_then(|ts| tile_tables.ctb_addr_ts_to_rs.get(ts as usize))
                .map(|&rs| rs as usize);
            let prev_slice = prev_rs.map(|rs| match &self.state {
                PictureStateEnum::U8(st) => st.tab_slice_addr_rs[rs],
                PictureStateEnum::U16(st) => st.tab_slice_addr_rs[rs],
            });
            if prev_slice != Some(recorded_slice_addr) {
                return Err(DecodeError::InvalidSyntax("previous slice segment missing"));
            }
        }
        let mut contexts = if sh.dependent_slice_segment_flag && slice_starts_tile {
            // The first CTB of a tile always starts from initialised contexts
            // (spec 9.3.1), even in a dependent slice segment.
            CabacContexts::init(sh.slice_qp_y, sh.slice_type, sh.cabac_init_flag)
        } else if sh.dependent_slice_segment_flag {
            // Does the dependent slice's first CTB start a CTB row of its
            // tile? Then WPP synchronises from the row above when the CTB
            // above-right is available (same tile and slice), and
            // initialises otherwise (spec 9.3.1).
            let w = pic_width_in_ctbs_for_init;
            let addr = sh.slice_segment_address;
            let col = addr % w;
            let on_row_start = col == tile_col_start[col as usize];
            if wpp_for_init && on_row_start {
                let parent_addr = job.recorded_slice_addr;
                let tr_available = addr >= w && col + 1 < w && {
                    let tr = (addr - w + 1) as usize;
                    let tr_slice = match &self.state {
                        PictureStateEnum::U8(st) => st.tab_slice_addr_rs[tr],
                        PictureStateEnum::U16(st) => st.tab_slice_addr_rs[tr],
                    };
                    tile_tables.tile_id[tr] == tile_tables.tile_id[addr as usize]
                        && tr_slice == parent_addr
                };
                if tr_available {
                    self.saved_wpp_cabac_state
                        .ok_or(DecodeError::InvalidSyntax(
                            "dependent WPP slice at row start without a saved WPP context state",
                        ))?
                } else {
                    CabacContexts::init(sh.slice_qp_y, sh.slice_type, sh.cabac_init_flag)
                }
            } else {
                self.saved_cabac_state.ok_or(DecodeError::InvalidSyntax(
                    "dependent slice segment without a saved CABAC state",
                ))?
            }
        } else {
            CabacContexts::init(sh.slice_qp_y, sh.slice_type, sh.cabac_init_flag)
        };

        with_picture_state!(&mut self.state, |state| {
            state.first_qp_group = job.segment_starts_qp_chain;
            if !sh.dependent_slice_segment_flag {
                state.last_qp_y = sh.slice_qp_y;
            }
            // CuQpOffsetCb / Cr start at 0 in every slice segment.
            state.cu_qp_offset_cb = 0;
            state.cu_qp_offset_cr = 0;
        });

        let wpp = pps.entropy_coding_sync_enabled_flag;

        let mut saved_state: Option<CabacContexts> = if sh.dependent_slice_segment_flag {
            self.saved_wpp_cabac_state
        } else {
            None
        };

        let mut cabac = CabacReader::new(&job.rbsp, cabac_byte_offset)?;

        let slice_ctx = crate::wavefront::SliceContext {
            sps,
            pps,
            sh,
            slice_params: &job.slice_params,
            recorded_slice_addr,
        };
        let substreams = crate::wavefront::Substreams {
            rbsp: &job.rbsp,
            data_start: cabac_byte_offset,
            nal_data_start: nal_slice_data_start,
            epb_positions,
            entry_point_offsets: &sh.entry_point_offsets,
        };
        let wavefront = threads > 1 && crate::wavefront::applies(sps, pps, sh, substreams.count());
        type CtbLoopResult = Result<(u32, Option<CabacContexts>, CabacContexts), DecodeError>;
        let ctb_loop_result: CtbLoopResult = with_picture_state!(&mut self.state, |state| {
            // A picture filtered row by row (frame threading) decodes its CTB
            // rows in order, the row filter following, so that the pictures
            // referencing it can start early; unless it has none to overlap
            // with (`rows_in_parallel`), when its rows decode in parallel
            // and it is filtered at its end.
            if wavefront && (state.row_filter.is_none() || self.rows_in_parallel) {
                // CTB rows on several threads; same result as the loop below.
                crate::wavefront::decode_rows(
                    state,
                    &slice_ctx,
                    &substreams,
                    slice_start_ts,
                    contexts,
                    saved_state,
                    threads,
                )
            } else {
                let mut more_data = true;
                let mut ctb_addr_ts: u32 = slice_start_ts;
                let mut substream_idx: u32 = 0;

                while more_data && ctb_addr_ts < total_ctbs {
                    let ctb_addr_rs = tile_tables.ctb_addr_ts_to_rs[ctb_addr_ts as usize];
                    let col = ctb_addr_rs % pic_width_in_ctbs;
                    let is_first_ctb_of_slice = ctb_addr_ts == slice_start_ts;

                    let is_tile_start = if is_first_ctb_of_slice {
                        false
                    } else {
                        let prev_ts = ctb_addr_ts - 1;
                        let prev_rs = tile_tables.ctb_addr_ts_to_rs[prev_ts as usize];
                        tile_tables.tile_id[ctb_addr_rs as usize]
                            != tile_tables.tile_id[prev_rs as usize]
                    };

                    // First CTB of a CTB row within its tile.
                    let is_row_start = col == tile_col_start[col as usize];
                    let needs_wpp_reinit = wpp && is_row_start && !is_first_ctb_of_slice;

                    if is_tile_start || needs_wpp_reinit {
                        substream_idx += 1;
                        let ep_idx = substream_idx as usize;
                        if ep_idx == 0 || ep_idx > sh.entry_point_offsets.len() {
                            return Err(DecodeError::InvalidSyntax(
                                "slice missing entry_point_offset for substream",
                            ));
                        }
                        let nal_offset_from_start = sh.entry_point_offsets[ep_idx - 1];
                        // Saturating: a damaged offset fails in reinit_at.
                        let nal_target = nal_slice_data_start.saturating_add(nal_offset_from_start);
                        let epbs_in_data_prefix = epb_positions
                            .iter()
                            .filter(|&&p| p >= nal_slice_data_start && p < nal_target)
                            .count() as u32;
                        let rbsp_offset_from_start = nal_offset_from_start - epbs_in_data_prefix;
                        let byte_offset = cabac_byte_offset + rbsp_offset_from_start as usize;
                        cabac.reinit_at(byte_offset)?;

                        // WPP synchronisation (spec 9.3.1): from the state saved
                        // after the second CTB of the row above when the CTB
                        // above-right is available (same tile and slice);
                        // otherwise, and at every tile start, initialise.
                        let tr_available = wpp
                            && ctb_addr_rs >= pic_width_in_ctbs
                            && col + 1 < pic_width_in_ctbs
                            && {
                                let tr = (ctb_addr_rs - pic_width_in_ctbs + 1) as usize;
                                tile_tables.tile_id[tr] == tile_tables.tile_id[ctb_addr_rs as usize]
                                    && state.tab_slice_addr_rs[tr] == recorded_slice_addr
                            };
                        if tr_available {
                            contexts = *saved_state.as_ref().ok_or(DecodeError::InvalidSyntax(
                                "WPP row start without a saved context state",
                            ))?;
                        } else {
                            contexts = CabacContexts::init(
                                sh.slice_qp_y,
                                sh.slice_type,
                                sh.cabac_init_flag,
                            );
                        }
                        state.first_qp_group = true;
                    }

                    // A complete CTB row is not rewritten (with frame
                    // threading it may be published and read by other
                    // pictures' threads); the same rule for any thread count.
                    if state.decoded_rows.frozen(ctb_addr_rs as usize) {
                        return Err(DecodeError::InvalidSyntax(
                            "slice segment overlaps CTB rows already decoded",
                        ));
                    }
                    let result = crate::wavefront::decode_ctb(
                        &mut cabac,
                        &mut contexts,
                        state,
                        &slice_ctx,
                        ctb_addr_rs,
                        ctb_addr_ts,
                    );
                    // A failed CTB stays undecoded for the row filter: its row
                    // is filtered at the picture's end, after the grey fill.
                    if result.is_ok()
                        && state.decoded_rows.mark(ctb_addr_rs as usize)
                        && let Some(mut filter) = state.row_filter.take()
                    {
                        filter.rows_decoded(state);
                        state.row_filter = Some(filter);
                    }
                    more_data = result?;
                    ctb_addr_ts += 1;

                    // Storage for WPP (spec 9.3.2.4): after the second CTB of a
                    // CTB row within its tile.
                    if wpp && col == tile_col_start[col as usize] + 1 {
                        saved_state = Some(contexts);
                    }
                }

                if more_data {
                    return Err(DecodeError::InvalidSyntax(
                        "slice did not end on terminate bin",
                    ));
                }

                Ok((ctb_addr_ts, saved_state, contexts))
            }
        });

        let (_, final_saved_state, final_contexts_state) = ctb_loop_result?;

        self.saved_cabac_state = Some(final_contexts_state);
        self.saved_wpp_cabac_state = final_saved_state;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct TileScanTables {
    /// Raster → tile-scan; converts `slice_segment_address` to the
    /// tile-scan position decoding starts at.
    ctb_addr_rs_to_ts: Vec<u32>,
    ctb_addr_ts_to_rs: Vec<u32>,
    tile_id: Vec<u32>,
}

impl TileScanTables {
    /// Derive the scan tables from an SPS and a PPS whose
    /// `resolve_tile_geometry` has already been called.
    fn build(sps: &Sps, pps: &Pps) -> Self {
        let pic_w_ctbs = sps.pic_width_in_ctbs_y() as usize;
        let pic_h_ctbs = sps.pic_height_in_ctbs_y() as usize;
        let total = pic_w_ctbs * pic_h_ctbs;

        let mut rs_to_ts = vec![0u32; total];
        let mut ts_to_rs = vec![0u32; total];
        let mut tile_id = vec![0u32; total];

        // Tile column / row boundaries in CTB coords (cumulative).
        let n_cols = pps.num_tile_columns;
        let n_rows = pps.num_tile_rows;
        let mut col_bd = vec![0u32; n_cols + 1];
        for i in 0..n_cols {
            col_bd[i + 1] = col_bd[i] + pps.column_widths_in_ctbs[i];
        }
        let mut row_bd = vec![0u32; n_rows + 1];
        for i in 0..n_rows {
            row_bd[i + 1] = row_bd[i] + pps.row_heights_in_ctbs[i];
        }

        // Fill `ctb_addr_rs_to_ts` via the formula in FFmpeg's `setup_pps`
        // (spec 6.5.1, HEVC reference decoder). For every raster address we
        // locate its tile (tile_x, tile_y) and count how many CTBs precede
        // it in tile-scan order.
        #[allow(clippy::needless_range_loop)]
        for ctb_addr_rs in 0..total {
            let tb_x = (ctb_addr_rs % pic_w_ctbs) as u32;
            let tb_y = (ctb_addr_rs / pic_w_ctbs) as u32;

            let mut tile_x = 0usize;
            for i in 0..n_cols {
                if tb_x < col_bd[i + 1] {
                    tile_x = i;
                    break;
                }
            }
            let mut tile_y = 0usize;
            for i in 0..n_rows {
                if tb_y < row_bd[i + 1] {
                    tile_y = i;
                    break;
                }
            }

            // Count CTBs in all earlier tiles within the same tile row (tile_y)
            // + all earlier tile rows.
            let mut val: u32 = 0;
            for i in 0..tile_x {
                val += pps.row_heights_in_ctbs[tile_y] * pps.column_widths_in_ctbs[i];
            }
            for i in 0..tile_y {
                val += (pic_w_ctbs as u32) * pps.row_heights_in_ctbs[i];
            }
            val += (tb_y - row_bd[tile_y]) * pps.column_widths_in_ctbs[tile_x]
                + (tb_x - col_bd[tile_x]);

            rs_to_ts[ctb_addr_rs] = val;
            if (val as usize) < total {
                ts_to_rs[val as usize] = ctb_addr_rs as u32;
            }
        }

        // Tile id per CTB raster address. Flattening by tile-row then
        // tile-column gives the standard tile ordering.
        let mut cur_id: u32 = 0;
        for j in 0..n_rows {
            for i in 0..n_cols {
                for y in row_bd[j]..row_bd[j + 1] {
                    for x in col_bd[i]..col_bd[i + 1] {
                        let rs = (y as usize) * pic_w_ctbs + x as usize;
                        if rs < total {
                            tile_id[rs] = cur_id;
                        }
                    }
                }
                cur_id += 1;
            }
        }

        Self {
            ctb_addr_rs_to_ts: rs_to_ts,
            ctb_addr_ts_to_rs: ts_to_rs,
            tile_id,
        }
    }
}

/// Resource limits applied before allocating storage for a decoded picture.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    /// Maximum coded luma samples in one picture.
    pub max_picture_pixels: u64,
    /// Maximum estimated bytes needed for picture state and frame emission.
    pub max_picture_memory_bytes: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_picture_pixels: 35_651_584,
            max_picture_memory_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Picture geometry that determines the min-TB Z-scan table: width,
/// height, log2 CTB size, log2 min TB size.
type ZscanKey = (u32, u32, u8, u8);

/// Streaming HEVC decoder.
///
/// Feed NAL units in bitstream order with [`decode_nal`](Self::decode_nal).
/// Parameter sets are stored as they arrive and activated by the slices
/// that use them.
///
/// Frames come out in one of two orders:
///
/// - **Decoding order** (the default, [`Decoder::new`]): `decode_nal`
///   returns each picture once it is decoded, and [`flush`](Self::flush)
///   returns any picture still in progress at the end of the stream.
/// - **Display order** ([`with_display_order`](Self::with_display_order)):
///   the spec's output process releases frames, taken with
///   [`next_output`](Self::next_output).
///
/// ```no_run
/// use deadlib_video_hevc::{Decoder, parse_annex_b};
///
/// let data = std::fs::read("input.h265")?;
/// let mut decoder = Decoder::new();
/// for nal in parse_annex_b(&data) {
///     if let Some(frame) = decoder.decode_nal(&nal)? {
///         // `frame` is in decoding order
///     }
/// }
/// while let Some(frame) = decoder.flush()? {
///     // last picture, if any
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors and damaged streams
///
/// An error from `decode_nal` concerns that NAL unit only (with
/// [`with_threads`](Self::with_threads) above 1, an error in a picture's
/// slice data may also come from a later call, or from
/// [`next_error`](Self::next_error)); keep feeding the stream. A picture that loses slices (failed or missing) is still
/// finished when the next picture starts, with the CTBs no slice decoded
/// filled mid-grey, and is output and used as a reference. A reference
/// picture that is missing altogether is replaced by a mid-grey stand-in
/// (spec 8.3.3). Pictures predicted from damaged ones show the damage until
/// the next IRAP picture. A picture whose first slice segment is lost is
/// dropped, as in FFmpeg.
///
/// `Decoder` is `Send`: it can be moved to another thread.
#[derive(Default)]
pub struct Decoder {
    limits: DecodeLimits,
    vps: Option<Vps>,
    sps: Option<Arc<Sps>>,
    pps: Option<Arc<Pps>>,
    /// Phase 3c-2: cached tile-scan tables for the active (SPS, PPS) pair.
    /// Rebuilt lazily on the first slice following a parameter-set change.
    tile_tables: Option<Arc<TileScanTables>>,
    /// The picture currently being assembled from one or more slice segments.
    /// Phase 3c-1: created on the first slice segment, finalized and
    /// returned as a `Frame` when all CTBs have been decoded.
    current_picture: Option<PictureInProgress>,
    /// Phase 3d-1: decoded picture buffer. Holds recently decoded pictures
    /// for reference lookup. Not yet used for inter decoding (which is the
    /// reason this infrastructure exists) — every picture currently
    /// inserted is an IDR intra picture. Output happens immediately after
    /// decoding; bumping for reorder is a future phase.
    dpb: DecodedPictureBuffer,
    /// Phase 3d-1: POC of the most recent decoded picture with
    /// `temporal_id == 0` (and not a sub-layer non-reference or
    /// RASL/RADL picture). Per spec 8.3.1 this is the "prevTid0Pic"
    /// seed for computing a non-IDR picture's POC from its LSB. Updated
    /// after every picture whose NAL type + temporal_id satisfy the
    /// "tid0" condition, IRAP pictures included (an IDR stores 0, a
    /// mid-stream CRA stores its real POC).
    prev_tid0_poc: i32,
    /// True once any picture has been decoded. Together with `after_eos`
    /// this decides whether an IRAP picture starts a new coded video
    /// sequence and so has NoRaslOutputFlag = 1 (spec 8.1.3): the first
    /// picture in the bitstream, or the first picture after an EOS NAL.
    /// IDR and BLA pictures always have NoRaslOutputFlag = 1 regardless.
    seen_picture: bool,
    /// True after an end-of-sequence NAL until the next picture starts.
    after_eos: bool,
    /// True while the RASL pictures associated with the most recent CRA
    /// must be skipped because that CRA had NoRaslOutputFlag = 1 (spec
    /// 8.1.3 / 8.3.2): their references precede the CRA and were never
    /// decoded. Cleared at the next IRAP picture.
    skip_rasl: bool,
    /// The last slice segment got through its header and reference lists
    /// (FFmpeg `slice_initialized`). A dependent slice segment needs it:
    /// otherwise it would inherit a failed or missing segment's state.
    slice_initialized: bool,
    /// Phase 3d-1: most recently derived reference picture sets for the
    /// current picture. Populated by `derive_rps_from_slice_header`; no
    /// consumer yet other than the decoder's own bookkeeping.
    current_rps: ReferencePictureSets,
    /// Phase 3d-2: RefPicList0 for the current P/B slice, built per spec
    /// 8.3.2 after RPS marking. Empty for I slices. Consumed by Phase 3d-5
    /// AMVP to populate `SliceParams::ref_pic_list_pocs`.
    current_ref_list_l0: Vec<Arc<DecodedPicture>>,
    /// Phase 3d-2: RefPicList1 for the current B slice. Empty for P and I
    /// slices. Consumed by Phase 3d-5 AMVP.
    current_ref_list_l1: Vec<Arc<DecodedPicture>>,

    /// Parameter sets received so far, indexed by their IDs (up to 16 SPSs
    /// and 64 PPSs). `sps` / `pps` hold the active pair, activated from
    /// these tables at the first slice of each picture.
    /// A set that failed to parse (e.g. it enables an unsupported coding
    /// tool) is kept as its error, so slices using that ID report it
    /// instead of decoding with an older set of the same ID.
    sps_table: Vec<Option<Result<Sps, DecodeError>>>,
    pps_table: Vec<Option<Result<Pps, DecodeError>>>,
    /// IDs of the active SPS / PPS, and whether a parameter set arrived
    /// since they were activated.
    active_sps_id: Option<u32>,
    active_pps_id: Option<u32>,
    parameter_sets_changed: bool,

    /// Min-TB Z-scan table shared by every picture with the same geometry,
    /// keyed by (width, height, log2 CTB size, log2 min TB size). Building
    /// it per picture was a measurable share of decode time.
    zscan_cache: Option<(ZscanKey, Arc<Vec<i32>>)>,

    /// Set by `with_display_order`: frames are released in output order by
    /// the spec C.5.2 output process and taken with `next_output`, instead
    /// of being returned by `decode_nal` in decoding order.
    display_order: bool,
    /// Set by `with_threads`: threads the picture-level passes may use (0
    /// from `Default` means 1).
    threads: usize,
    /// Frame threading (`threads > 1`): pictures whose last slice has been
    /// handed to their thread, oldest first.
    in_flight: std::collections::VecDeque<InFlight>,
    /// Errors from the slice data of pictures decoded on their own threads,
    /// returned by later calls to `decode_nal`.
    deferred_errors: std::collections::VecDeque<DecodeError>,
    next_picture_id: u64,
    /// Frame threading: the threads pictures decode on (created with the
    /// first picture).
    workers: Option<crate::parallel::ThreadPool>,
    output_queue: crate::output::OutputQueue,
    /// Decoding-order mode: frames ready but not yet returned. A call can
    /// finish two pictures (an incomplete earlier one, then its own), and
    /// `decode_nal` returns one frame per call.
    pending_frames: std::collections::VecDeque<Frame>,
    /// Buffers reused from earlier pictures.
    pool: PicturePool,
}

/// Fill every CTB that no slice decoded (`tab_slice_addr_rs == -1`) with
/// mid-grey, the value the spec uses for generated unavailable pictures
/// (8.3.3.2). Such CTBs come from lost or failed slices; without this they
/// would hold zeros, which show as saturated green and spread through
/// motion compensation.
fn fill_undecoded_ctbs<P: Pixel>(state: &mut crate::cu_tree::PictureState<P>) {
    let ctb = 1usize << state.log2_ctb_size;
    let (w, h) = (state.width as usize, state.height as usize);
    let ctb_w = w.div_ceil(ctb);
    let mid = |bd: u8| P::from_i32_clamped(1 << (bd - 1), bd);
    let (mid_y, mid_c) = (mid(state.bit_depth), mid(state.bit_depth_c));
    let (sw, sh) = (state.chroma_shift_w, state.chroma_shift_h);
    for rs in 0..state.tab_slice_addr_rs.len() {
        if state.tab_slice_addr_rs[rs] != -1 {
            continue;
        }
        let (x0, y0) = ((rs % ctb_w) * ctb, (rs / ctb_w) * ctb);
        let (x1, y1) = ((x0 + ctb).min(w), (y0 + ctb).min(h));
        let ys = state.y_stride;
        for y in y0..y1 {
            state.y_plane[y * ys + x0..y * ys + x1].fill(mid_y);
        }
        if state.chroma_array_type != 0 {
            let cs = state.uv_stride;
            let (cx0, cx1) = (x0 >> sw, x1.div_ceil(1 << sw));
            for y in (y0 >> sh)..y1.div_ceil(1 << sh) {
                state.u_plane[y * cs + cx0..y * cs + cx1].fill(mid_c);
                state.v_plane[y * cs + cx0..y * cs + cx1].fill(mid_c);
            }
        }
    }
}

/// Min-TB Z-scan table for `sps`, shared through `cache` while the picture
/// geometry stays the same.
fn zscan_table(cache: &mut Option<(ZscanKey, Arc<Vec<i32>>)>, sps: &Sps) -> Arc<Vec<i32>> {
    let key = (
        sps.pic_width_in_luma_samples,
        sps.pic_height_in_luma_samples,
        sps.ctb_log2_size_y,
        sps.min_tb_log2_size_y,
    );
    match cache {
        Some((k, tab)) if *k == key => Arc::clone(tab),
        _ => {
            let tab = Arc::new(crate::cu_tree::build_min_tb_addr_zs(sps));
            *cache = Some((key, Arc::clone(&tab)));
            tab
        }
    }
}

/// Buffers kept from earlier pictures so that a new picture does not
/// allocate (and fault in, and zero) its planes and tables from scratch.
#[derive(Default)]
struct PicturePool {
    /// State of the last finished picture, per sample type.
    state_u8: Option<PictureState<u8>>,
    state_u16: Option<PictureState<u16>>,
    /// Planes and motion fields of pictures that left the DPB.
    planes_u8: Vec<([Vec<u8>; 3], Vec<MvField>)>,
    planes_u16: Vec<([Vec<u16>; 3], Vec<MvField>)>,
}

/// Plane sets kept per sample type; about one picture leaves the DPB per
/// picture decoded, so two is enough.
const POOLED_PLANE_SETS: usize = 2;

impl PicturePool {
    /// A picture state for `sps`, built from pooled buffers when available.
    fn new_state(&mut self, sps: &Sps, zscan: Arc<Vec<i32>>) -> PictureStateEnum {
        if sps.needs_u16_samples() {
            let (planes, mvf) = self
                .planes_u16
                .pop()
                .map_or((None, None), |(p, m)| (Some(p), Some(m)));
            let recycled = RecycledBuffers {
                state: self.state_u16.take(),
                planes,
                mvf,
            };
            PictureStateEnum::U16(PictureState::new_recycled(sps, zscan, recycled))
        } else {
            let (planes, mvf) = self
                .planes_u8
                .pop()
                .map_or((None, None), |(p, m)| (Some(p), Some(m)));
            let recycled = RecycledBuffers {
                state: self.state_u8.take(),
                planes,
                mvf,
            };
            PictureStateEnum::U8(PictureState::new_recycled(sps, zscan, recycled))
        }
    }

    /// Keep a finished picture's state (its planes were moved out).
    fn recycle_state(&mut self, state: PictureStateEnum) {
        match state {
            PictureStateEnum::U8(s) => self.state_u8 = Some(s),
            PictureStateEnum::U16(s) => self.state_u16 = Some(s),
        }
    }

    /// Take back the planes and motion field of a picture that left the
    /// DPB, unless something else (a reference list) still holds it.
    fn recycle_picture(&mut self, pic: Arc<DecodedPicture>) {
        let Ok(pic) = Arc::try_unwrap(pic) else {
            return;
        };
        let mvf = pic.tab_mvf.into_vec();
        match pic.planes {
            crate::dpb::PicturePlanes::U8(p) if self.planes_u8.len() < POOLED_PLANE_SETS => {
                self.planes_u8.push((p.map(SharedTable::into_vec), mvf));
            }
            crate::dpb::PicturePlanes::U16(p) if self.planes_u16.len() < POOLED_PLANE_SETS => {
                self.planes_u16.push((p.map(SharedTable::into_vec), mvf));
            }
            _ => {}
        }
    }
}

impl Drop for Decoder {
    /// Pictures still decoding end with the decoder: the current picture's
    /// task sees its channel close, the others finish; then the worker
    /// threads are joined.
    fn drop(&mut self) {
        self.current_picture = None;
        for f in self.in_flight.drain(..) {
            let _ = f.handle.join();
        }
        self.workers = None;
    }
}

/// A fresh picture state with the sample type the SPS needs.
fn new_picture_state(sps: &Sps, zscan: Arc<Vec<i32>>) -> PictureStateEnum {
    if sps.needs_u16_samples() {
        PictureStateEnum::U16(PictureState::<u16>::new_with_zscan(sps, zscan))
    } else {
        PictureStateEnum::U8(PictureState::<u8>::new_with_zscan(sps, zscan))
    }
}

/// A mid-grey reference picture with POC `poc` and no motion (spec
/// 8.3.3.2), standing in for a reference that is missing from the DPB.
fn generated_reference<P: Pixel>(
    state: &mut crate::cu_tree::PictureState<P>,
    poc: i32,
) -> Arc<DecodedPicture> {
    fill_undecoded_ctbs(state);
    let (width, height) = (state.width, state.height);
    let (y_stride, uv_stride) = (state.y_stride, state.uv_stride);
    let (chroma_shift_w, chroma_shift_h) = (state.chroma_shift_w, state.chroma_shift_h);
    let (y, u, v, tab_mvf, log2_min_pu_size, min_pu_width, log2_ctb_size) =
        state.take_planes_and_mvf();
    let mut dp = DecodedPicture::new_with_mvf(
        y,
        u,
        v,
        width,
        height,
        poc,
        tab_mvf,
        log2_min_pu_size,
        min_pu_width,
        log2_ctb_size,
        [Vec::new(), Vec::new()],
    );
    dp.y_stride = y_stride;
    dp.uv_stride = uv_stride;
    dp.chroma_shift_w = chroma_shift_w;
    dp.chroma_shift_h = chroma_shift_h;
    Arc::new(dp)
}

/// Crop a decoded picture's planes to the conformance window and build a Frame.
/// The pixel planes may have CTU-aligned strides larger than `coded_w`, so we
/// always use `state.y_stride` / `state.uv_stride` for row addressing.
#[allow(clippy::too_many_arguments)]
fn crop_frame<P: Pixel>(
    state: &crate::cu_tree::PictureState<P>,
    coded_w: u32,
    coded_h: u32,
    cropped_w: u32,
    cropped_h: u32,
    left_offset: u32, // in chroma sample units
    top_offset: u32,  // in chroma sample units
    poc: i32,
    bit_depth: u8,
) -> Result<Frame, DecodeError> {
    // Conformance-window offsets are in chroma sample units; scale by
    // SubWidthC/SubHeightC (`chroma_shift_*`) to reach luma coordinates.
    let luma_left = left_offset
        .checked_shl(state.chroma_shift_w)
        .filter(|v| v >> state.chroma_shift_w == left_offset)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(DecodeError::InvalidSyntax("invalid luma crop offset"))?;
    let luma_top = top_offset
        .checked_shl(state.chroma_shift_h)
        .filter(|v| v >> state.chroma_shift_h == top_offset)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(DecodeError::InvalidSyntax("invalid luma crop offset"))?;
    let cw = usize::try_from(cropped_w)
        .map_err(|_| DecodeError::InvalidSyntax("invalid cropped width"))?;
    let ch = usize::try_from(cropped_h)
        .map_err(|_| DecodeError::InvalidSyntax("invalid cropped height"))?;
    let coded_w =
        usize::try_from(coded_w).map_err(|_| DecodeError::InvalidSyntax("invalid coded width"))?;
    let coded_h =
        usize::try_from(coded_h).map_err(|_| DecodeError::InvalidSyntax("invalid coded height"))?;
    if luma_left.checked_add(cw).is_none_or(|end| end > coded_w)
        || luma_top.checked_add(ch).is_none_or(|end| end > coded_h)
    {
        return Err(DecodeError::InvalidSyntax(
            "conformance window exceeds picture dimensions",
        ));
    }
    let stride_y = state.y_stride;
    let stride_uv = state.uv_stride;
    let chroma_format_idc = state.chroma_array_type;

    // Fast path: if the output dimensions match the stride (no padding, no
    // conformance crop), we can do a simple row-copy or even clone.
    let no_crop =
        luma_left == 0 && luma_top == 0 && cw == stride_y && ch == state.y_plane.len() / stride_y;
    if no_crop {
        return Ok(Frame {
            y: P::wrap_vec(state.y_plane[0..state.y_plane.len()].to_vec()),
            u: P::wrap_vec(state.u_plane[0..state.u_plane.len()].to_vec()),
            v: P::wrap_vec(state.v_plane[0..state.v_plane.len()].to_vec()),
            width: cropped_w,
            height: cropped_h,
            pic_order_cnt: poc,
            bit_depth,
            chroma_format_idc,
        });
    }

    let mut y = Vec::with_capacity(cw * ch);
    for row in 0..ch {
        let src_row = luma_top + row;
        let start = src_row
            .checked_mul(stride_y)
            .and_then(|value| value.checked_add(luma_left))
            .ok_or(DecodeError::InvalidSyntax("invalid luma crop range"))?;
        let end = start
            .checked_add(cw)
            .ok_or(DecodeError::InvalidSyntax("invalid luma crop range"))?;
        y.extend_from_slice(
            state
                .y_plane
                .get_range(start..end)
                .ok_or(DecodeError::InvalidSyntax("invalid luma crop range"))?,
        );
    }
    let cw_c = cw >> state.chroma_shift_w;
    let ch_c = ch >> state.chroma_shift_h;
    let chroma_left = usize::try_from(left_offset)
        .map_err(|_| DecodeError::InvalidSyntax("invalid chroma crop offset"))?;
    let chroma_top = usize::try_from(top_offset)
        .map_err(|_| DecodeError::InvalidSyntax("invalid chroma crop offset"))?;
    let mut u = Vec::with_capacity(cw_c * ch_c);
    let mut v = Vec::with_capacity(cw_c * ch_c);
    for row in 0..ch_c {
        let src_row = chroma_top + row;
        let start = src_row
            .checked_mul(stride_uv)
            .and_then(|value| value.checked_add(chroma_left))
            .ok_or(DecodeError::InvalidSyntax("invalid chroma crop range"))?;
        let end = start
            .checked_add(cw_c)
            .ok_or(DecodeError::InvalidSyntax("invalid chroma crop range"))?;
        u.extend_from_slice(
            state
                .u_plane
                .get_range(start..end)
                .ok_or(DecodeError::InvalidSyntax("invalid chroma crop range"))?,
        );
        v.extend_from_slice(
            state
                .v_plane
                .get_range(start..end)
                .ok_or(DecodeError::InvalidSyntax("invalid chroma crop range"))?,
        );
    }
    Ok(Frame {
        y: P::wrap_vec(y),
        u: P::wrap_vec(u),
        v: P::wrap_vec(v),
        width: cropped_w,
        height: cropped_h,
        pic_order_cnt: poc,
        bit_depth,
        chroma_format_idc,
    })
}

impl Decoder {
    /// A decoder in decoding-order mode with the default
    /// [`DecodeLimits`]: Level 6.2's maximum picture size and an estimated
    /// 512 MB of state per picture.
    ///
    /// Most callers should chain [`with_display_order`](Self::with_display_order):
    /// decoding order is the default only for compatibility.
    pub fn new() -> Self {
        Self::default()
    }

    /// A decoder in decoding-order mode with custom resource limits. An SPS
    /// that exceeds them is rejected with [`DecodeError::ResourceLimit`].
    pub fn with_limits(limits: DecodeLimits) -> Self {
        let mut decoder = Self::default();
        decoder.limits = limits;
        decoder
    }

    /// Output frames in display order, as the spec's output process does
    /// (C.5.2, "bumping"), instead of in decoding order. This is the
    /// recommended mode: it handles reordering, `pic_output_flag` and
    /// sequence boundaries that a caller sorting decoding-order frames by
    /// POC would have to get right itself.
    ///
    /// In this mode `decode_nal` always returns `Ok(None)`. Take the frames
    /// released so far with [`next_output`](Self::next_output) after each
    /// call, and at the end of the stream call [`flush`](Self::flush) until
    /// it returns `None`. The decoder applies the stream's reorder and DPB
    /// size limits, and drops pictures the spec does not output: those with
    /// `pic_output_flag = 0`, skipped RASL pictures, and pictures still
    /// waiting when an IRAP picture with `NoOutputOfPriorPicsFlag = 1`
    /// starts a new coded video sequence.
    ///
    /// ```no_run
    /// # let data: Vec<u8> = Vec::new();
    /// let mut dec = deadlib_video_hevc::Decoder::new().with_display_order();
    /// for nal in deadlib_video_hevc::parse_annex_b(&data) {
    ///     dec.decode_nal(&nal)?;
    ///     while let Some(frame) = dec.next_output() {
    ///         // display `frame`
    ///     }
    /// }
    /// while let Some(frame) = dec.flush()? {
    ///     // display `frame`
    /// }
    /// # Ok::<(), deadlib_video_hevc::DecodeError>(())
    /// ```
    pub fn with_display_order(mut self) -> Self {
        self.display_order = true;
        self
    }

    /// Use up to `threads` threads (at least 1; the default is 1).
    ///
    /// The frames, their order and every sample are identical for every
    /// thread count; only the speed and the timing change:
    ///
    /// - Frame threading: each picture decodes on its own thread, up to
    ///   `threads` at a time. A picture's loop filters run row by row as its
    ///   CTB rows finish, so the pictures referencing it start right away:
    ///   motion compensation and temporal MV prediction wait only for the
    ///   rows they read. Works for every stream.
    /// - With wavefront parallel processing (`entropy_coding_sync_enabled_flag`,
    ///   on by default in x265), a picture with no other picture to overlap
    ///   with (the first one, or a still image) decodes its CTB rows in
    ///   parallel, each row two CTBs behind the one above.
    ///
    /// With more than one thread the decoder works ahead of the caller: a
    /// frame comes out of a later `decode_nal` / [`next_output`](Self::next_output)
    /// call than with one thread (and the rest from [`flush`](Self::flush)),
    /// and an error in a picture's slice data is returned by a later
    /// `decode_nal` call, or by [`next_error`](Self::next_error) after the
    /// last one. Errors in NAL unit headers, slice headers and parameter
    /// sets still come from the call that received them.
    ///
    /// [`std::thread::available_parallelism`] is a reasonable value; more
    /// threads than the machine's performance cores rarely help.
    ///
    /// ```
    /// let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    /// let decoder = deadlib_video_hevc::Decoder::new().with_display_order().with_threads(threads);
    /// # drop(decoder);
    /// ```
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads.max(1);
        self
    }

    /// Next frame released for display, in display-order mode (see
    /// [`with_display_order`](Self::with_display_order)). Always `None` in
    /// the default decoding-order mode. With
    /// [`with_threads`](Self::with_threads) above 1 it is also `None` while
    /// the next frame in display order is still decoding: keep calling
    /// after each NAL unit, and [`flush`](Self::flush) at the end.
    pub fn next_output(&mut self) -> Option<Frame> {
        self.collect_pictures(false);
        self.output_queue.pop()
    }

    /// The next error from a picture decoded on its own thread that no
    /// `decode_nal` call has returned yet. With
    /// [`with_threads`](Self::with_threads) above 1, errors in a picture's
    /// slice data are found after the call that fed the slice;
    /// `decode_nal` returns them on later calls, and this returns those
    /// still waiting, for example after [`flush`](Self::flush). Always
    /// `None` with one thread.
    pub fn next_error(&mut self) -> Option<DecodeError> {
        self.collect_pictures(false);
        self.deferred_errors.pop_front()
    }

    fn validate_picture_limits(&self, sps: &Sps) -> Result<(), DecodeError> {
        let pixels = u64::from(sps.pic_width_in_luma_samples)
            .checked_mul(u64::from(sps.pic_height_in_luma_samples))
            .ok_or(DecodeError::ResourceLimit("picture pixel count overflow"))?;
        if pixels > self.limits.max_picture_pixels {
            return Err(DecodeError::ResourceLimit("picture pixel count"));
        }

        let ctb_size = 1u64 << sps.ctb_log2_size_y;
        let width = u64::from(sps.pic_width_in_luma_samples).div_ceil(ctb_size) * ctb_size;
        let height = u64::from(sps.pic_height_in_luma_samples).div_ceil(ctb_size) * ctb_size;
        let aligned_pixels = width.checked_mul(height).ok_or(DecodeError::ResourceLimit(
            "picture memory estimate overflow",
        ))?;
        let pixel_bytes = if sps.needs_u16_samples() { 2 } else { 1 };
        let plane_bytes =
            aligned_pixels
                .checked_mul(3 * pixel_bytes)
                .ok_or(DecodeError::ResourceLimit(
                    "picture memory estimate overflow",
                ))?;
        let min_pu_count = aligned_pixels >> 4;
        let min_cb_count = aligned_pixels >> (2 * sps.min_cb_log2_size_y);
        let min_tb_count = aligned_pixels >> (2 * sps.min_tb_log2_size_y);
        let ctb_count = (width / ctb_size) * (height / ctb_size);
        let table_bytes = min_pu_count
            .checked_mul((std::mem::size_of::<MvField>() + 1) as u64)
            .and_then(|n| n.checked_add(min_cb_count * 3))
            .and_then(|n| n.checked_add(aligned_pixels / 8))
            .and_then(|n| n.checked_add(min_tb_count * 5))
            .and_then(|n| {
                n.checked_add(
                    ctb_count * (std::mem::size_of::<crate::sao::SaoParams>() as u64 + 18),
                )
            })
            .ok_or(DecodeError::ResourceLimit(
                "picture memory estimate overflow",
            ))?;
        let estimated_bytes =
            plane_bytes
                .checked_add(table_bytes)
                .ok_or(DecodeError::ResourceLimit(
                    "picture memory estimate overflow",
                ))?;
        if estimated_bytes > self.limits.max_picture_memory_bytes {
            return Err(DecodeError::ResourceLimit("estimated picture memory"));
        }
        Ok(())
    }

    /// Feed one NAL unit. Returns `Ok(Some(frame))` when a picture has just
    /// finished decoding, `Ok(None)` otherwise (e.g. parameter sets, SEI).
    ///
    /// At most one frame is returned per call. When a damaged picture is
    /// finished at the start of the next one and that next picture also
    /// completes in the same call, the later frame is returned by the next
    /// call (or by `flush`), so frames stay in decoding order.
    pub fn decode_nal(&mut self, nal: &NalUnit<'_>) -> Result<Option<Frame>, DecodeError> {
        let result = self.decode_nal_inner(nal);
        self.collect_pictures(false);
        // An error from a picture decoded on its own thread (frame
        // threading) comes out after this NAL unit's own error.
        if result.is_ok()
            && let Some(e) = self.deferred_errors.pop_front()
        {
            if let Ok(Some(frame)) = result {
                self.pending_frames.push_back(frame);
            }
            return Err(e);
        }
        if self.pending_frames.is_empty() {
            return result;
        }
        // Frames finished earlier go out first, keeping decoding order.
        match result {
            Ok(Some(frame)) => {
                self.pending_frames.push_back(frame);
                Ok(self.pending_frames.pop_front())
            }
            Ok(None) => Ok(self.pending_frames.pop_front()),
            Err(e) => Err(e),
        }
    }

    /// Accepts a configuration NAL while leaving queued output for the caller.
    pub(crate) fn decode_config_nal(&mut self, nal: &NalUnit<'_>) -> Result<(), DecodeError> {
        match nal.nal_unit_type {
            NalUnitType::Vps
            | NalUnitType::Sps
            | NalUnitType::Pps
            | NalUnitType::PrefixSei
            | NalUnitType::SuffixSei => self.decode_nal_inner(nal).map(|_| ()),
            _ => Err(DecodeError::Unsupported("HEVC configuration NAL unit type")),
        }
    }

    fn decode_nal_inner(&mut self, nal: &NalUnit<'_>) -> Result<Option<Frame>, DecodeError> {
        // Spec 7.4.2.2: a decoder of the base layer ignores NAL units with
        // nuh_layer_id > 0 (the layers of SHVC / MV-HEVC streams, such as
        // the second view of stereo video), parameter sets included.
        if nal.nuh_layer_id > 0 {
            return Ok(None);
        }
        match nal.nal_unit_type {
            NalUnitType::Vps => {
                self.vps = Some(parse_vps(&nal.rbsp)?);
                Ok(None)
            }
            NalUnitType::Sps => {
                let parsed = parse_sps(&nal.rbsp).and_then(|sps| {
                    self.validate_picture_limits(&sps)?;
                    Ok(sps)
                });
                let id = match &parsed {
                    Ok(sps) => Some(sps.sps_seq_parameter_set_id),
                    Err(_) => crate::sps::peek_sps_id(&nal.rbsp),
                };
                let err = parsed.as_ref().err().cloned();
                let id = match id {
                    Some(id) if id <= 15 => id as usize,
                    _ => {
                        return Err(err.unwrap_or(DecodeError::InvalidSyntax(
                            "sps_seq_parameter_set_id > 15",
                        )));
                    }
                };
                if self.sps_table.len() <= id {
                    self.sps_table.resize(id + 1, None);
                }
                // A rejected SPS replaces any earlier one with its ID.
                self.sps_table[id] = Some(parsed);
                self.parameter_sets_changed = true;
                err.map_or(Ok(None), Err)
            }
            NalUnitType::Pps => {
                let parsed = parse_pps(&nal.rbsp);
                let id = match &parsed {
                    Ok(pps) => Some(pps.pps_pic_parameter_set_id),
                    Err(_) => crate::pps::peek_pps_id(&nal.rbsp),
                };
                let err = parsed.as_ref().err().cloned();
                let id = match id {
                    Some(id) if id <= 63 => id as usize,
                    _ => {
                        return Err(err.unwrap_or(DecodeError::InvalidSyntax(
                            "pps_pic_parameter_set_id > 63",
                        )));
                    }
                };
                if self.pps_table.len() <= id {
                    self.pps_table.resize(id + 1, None);
                }
                // A rejected PPS replaces any earlier one with its ID.
                self.pps_table[id] = Some(parsed);
                self.parameter_sets_changed = true;
                err.map_or(Ok(None), Err)
            }
            NalUnitType::Eos => {
                // End of sequence: the next picture must be an IRAP and
                // starts a new coded video sequence (NoRaslOutputFlag = 1).
                let frame = self.end_picture()?;
                self.after_eos = true;
                Ok(frame)
            }
            // An access unit delimiter or end of bitstream also ends the
            // picture before it, so its frame need not wait for the next
            // picture.
            NalUnitType::Aud | NalUnitType::Eob => self.end_picture(),
            t if t.is_vcl() => self.decode_slice(nal, t),
            _ => Ok(None),
        }
    }

    /// End the stream: finish the picture in progress and return the frames
    /// still waiting, one per call, until `None`. Call it at the end of
    /// every stream.
    ///
    /// A picture ends when the next one starts, at an access unit
    /// delimiter, end of sequence or end of bitstream NAL unit, or here: its
    /// slices can arrive over several `decode_nal` calls, so the last
    /// picture of a stream without such a NAL unit after it comes out of
    /// `flush`. With [`with_threads`](Self::with_threads) above 1 this also
    /// waits for the pictures still decoding.
    ///
    /// In display-order mode this ends the stream: every frame still
    /// waiting is released, and each call returns the next one in display
    /// order until `None`.
    pub fn flush(&mut self) -> Result<Option<Frame>, DecodeError> {
        // A picture still in progress at the end of the stream (its last
        // slices lost or failed) is finished with its missing CTBs filled.
        if let Some(frame) = self.end_picture()? {
            self.pending_frames.push_back(frame);
        }
        // Frame threading: every picture still decoding finishes first.
        self.collect_pictures(true);
        if let Some(error) = self.deferred_errors.pop_front() {
            return Err(error);
        }
        if self.display_order {
            self.output_queue.output_all();
            return Ok(self.output_queue.pop());
        }
        Ok(self.pending_frames.pop_front())
    }

    /// End the picture in progress, if any (it is finished as it is, with
    /// any CTBs no slice decoded filled mid-grey).
    fn end_picture(&mut self) -> Result<Option<Frame>, DecodeError> {
        match self.current_picture.take() {
            Some(pic) => self.finish_picture(pic),
            None => Ok(None),
        }
    }

    fn decode_slice(
        &mut self,
        nal: &NalUnit<'_>,
        nut: NalUnitType,
    ) -> Result<Option<Frame>, DecodeError> {
        // Cleared until this segment's reference lists are built, whatever
        // happens to it before then (as FFmpeg does).
        let previous_slice_initialized = std::mem::replace(&mut self.slice_initialized, false);
        // first_slice_segment_in_pic_flag is the slice header's first bit.
        if nal.rbsp.first().is_some_and(|b| b & 0x80 != 0) {
            // The previous picture never completed: slices were lost or
            // failed to decode. Finish it as it is (with the parameter sets
            // it was decoded with), so it can still be output and used as a
            // reference, and start the new picture.
            if let Some(pic) = self.current_picture.take()
                && let Some(frame) = self.finish_picture(pic)?
            {
                self.pending_frames.push_back(frame);
            }
            self.activate_parameter_sets(nal, nut)?;
        }

        // Phase 3c-2: resolve PPS tile geometry and cache the tile-scan
        // tables up front. We need `&mut self.pps` for `resolve_tile_geometry`
        // but `&self.sps` for the inputs — take the SPS out of the option
        // temporarily via a clone of the reference.
        {
            let sps = self
                .sps
                .as_ref()
                .ok_or(DecodeError::InvalidSyntax("slice without active SPS"))?;
            let pps = self
                .pps
                .as_mut()
                .ok_or(DecodeError::InvalidSyntax("slice without active PPS"))?;
            if pps.column_widths_in_ctbs.is_empty() {
                // Resolved once per activation, before any slice of it is
                // handed to a picture thread (otherwise this clones).
                Arc::make_mut(pps).resolve_tile_geometry(sps)?;
            }
            if self.tile_tables.is_none() {
                self.tile_tables = Some(Arc::new(TileScanTables::build(sps, pps)));
            }
        }

        let sps = self.sps.as_ref().expect("sps present above");
        let pps = self.pps.as_ref().expect("pps present above");
        let tile_tables = self.tile_tables.as_ref().expect("tile tables built above");

        let mut sh = parse_slice_segment_header(&nal.rbsp, nut, sps, pps)?;
        sh.temporal_id = nal.temporal_id;
        if sh.dependent_slice_segment_flag && !previous_slice_initialized {
            return Err(DecodeError::InvalidSyntax(
                "independent slice segment missing",
            ));
        }
        if !sh.first_slice_segment_in_pic_flag
            && let Some(pic) = self.current_picture.as_ref()
            && pic.nal_unit_type != nut
        {
            return Err(DecodeError::InvalidSyntax(
                "slice NAL unit type differs from the picture's first slice",
            ));
        }

        // Phase 3c-4: a dependent slice segment inherits the slice-header
        // fields that the parser deliberately left at their defaults —
        // slice_type, slice_qp_y, slice_sao_*, deblock override, ... —
        // from the most recent independent slice segment in this picture.
        // The entry_point_offsets / slice_segment_address / header_size_bits
        // are independently signaled in the dependent slice's own header
        // and must NOT be overwritten.
        if sh.dependent_slice_segment_flag {
            let pic = self
                .current_picture
                .as_ref()
                .ok_or(DecodeError::InvalidSyntax(
                    "dependent slice segment without an active picture",
                ))?;
            let parent = &pic.last_independent_slice_header;
            sh.slice_type = parent.slice_type;
            sh.pic_output_flag = parent.pic_output_flag;
            sh.slice_pic_order_cnt_lsb = parent.slice_pic_order_cnt_lsb;
            sh.slice_sao_luma_flag = parent.slice_sao_luma_flag;
            sh.slice_sao_chroma_flag = parent.slice_sao_chroma_flag;
            sh.slice_qp_delta = parent.slice_qp_delta;
            sh.slice_qp_y = parent.slice_qp_y;
            sh.slice_cb_qp_offset = parent.slice_cb_qp_offset;
            sh.slice_cr_qp_offset = parent.slice_cr_qp_offset;
            sh.cu_chroma_qp_offset_enabled_flag = parent.cu_chroma_qp_offset_enabled_flag;
            sh.slice_deblocking_filter_disabled_flag = parent.slice_deblocking_filter_disabled_flag;
            sh.slice_beta_offset_div2 = parent.slice_beta_offset_div2;
            sh.slice_tc_offset_div2 = parent.slice_tc_offset_div2;
            // Inherit inter bitstream fields too, since a dependent slice
            // segment shares the logical slice of its parent.
            sh.short_term_ref_pic_set_sps_flag = parent.short_term_ref_pic_set_sps_flag;
            sh.short_term_ref_pic_set_idx = parent.short_term_ref_pic_set_idx;
            sh.short_term_rps = parent.short_term_rps.clone();
            sh.long_term_rps = parent.long_term_rps.clone();
            sh.slice_temporal_mvp_enabled_flag = parent.slice_temporal_mvp_enabled_flag;
            sh.num_ref_idx_l0_active_minus1 = parent.num_ref_idx_l0_active_minus1;
            sh.num_ref_idx_l1_active_minus1 = parent.num_ref_idx_l1_active_minus1;
            sh.mvd_l1_zero_flag = parent.mvd_l1_zero_flag;
            sh.cabac_init_flag = parent.cabac_init_flag;
            sh.collocated_from_l0_flag = parent.collocated_from_l0_flag;
            sh.collocated_ref_idx = parent.collocated_ref_idx;
            sh.max_num_merge_cand = parent.max_num_merge_cand;
            sh.ref_pic_list_modification = parent.ref_pic_list_modification.clone();
            sh.pred_weight_table = parent.pred_weight_table.clone();
            sh.slice_loop_filter_across_slices_enabled_flag =
                parent.slice_loop_filter_across_slices_enabled_flag;
            sh.poc = parent.poc;
        }

        // Phase 3d-1: compute POC from the slice header's pic_order_cnt_lsb.
        // IDR pictures always get POC = 0 (and the spec says prev_tid0_poc
        // is reset too). For non-IDR pictures we use the `compute_poc`
        // helper with the current `prev_tid0_poc` seed. Dependent slice
        // segments inherit the POC from the parent independent segment
        // (populated above) — we only compute it once per picture (on the
        // first, independent slice).
        let mut starts_new_sequence = false;
        let mut irap_no_rasl_output = false;
        if sh.first_slice_segment_in_pic_flag {
            // Spec 8.1.3: an IRAP picture has NoRaslOutputFlag = 1 when it
            // is IDR, BLA, the first picture in the bitstream, or the first
            // picture after an EOS NAL. Such a picture starts a new coded
            // video sequence: its POC MSB is 0 (8.3.1) and, for a CRA, the
            // associated RASL pictures are not decoded (8.1.3).
            let no_rasl_output_flag =
                nut.is_irap() && (!nut.is_cra() || !self.seen_picture || self.after_eos);
            // C.5.2.2: an IRAP with NoRaslOutputFlag = 1 other than the
            // first picture ends output of the previous sequence.
            starts_new_sequence = no_rasl_output_flag && self.seen_picture;
            irap_no_rasl_output = no_rasl_output_flag;
            if nut.is_irap() {
                // RASL pictures are skipped after any IRAP with NoRaslOutputFlag
                // = 1: a BLA always, a CRA that starts decoding (spec 8.1.3).
                // IDR pictures have no associated RASL pictures.
                self.skip_rasl = no_rasl_output_flag;
            }
            self.seen_picture = true;
            self.after_eos = false;
            if matches!(nut, NalUnitType::RaslN | NalUnitType::RaslR) && self.skip_rasl {
                return Ok(None);
            }
            if nut.is_idr() {
                sh.poc = 0;
            } else {
                sh.poc = Self::compute_poc(
                    self.prev_tid0_poc,
                    sh.slice_pic_order_cnt_lsb,
                    sps.log2_max_pic_order_cnt_lsb,
                    no_rasl_output_flag,
                )
                .ok_or(DecodeError::InvalidSyntax("PicOrderCntVal out of range"))?;
            }
        } else if matches!(nut, NalUnitType::RaslN | NalUnitType::RaslR) && self.skip_rasl {
            // Remaining slice segments of a skipped RASL picture.
            return Ok(None);
        } else if !sh.dependent_slice_segment_flag {
            // A non-first independent slice segment must have the same POC
            // as the rest of the picture; just take it from the current
            // picture in progress.
            if let Some(pic) = self.current_picture.as_ref() {
                sh.poc = pic.last_independent_slice_header.poc;
            }
        }

        // Phase 3c-4: CABAC context setup.
        //
        // - Independent slice segment → fresh `CabacContexts::init` from
        //   the slice QP and slice type, matching FFmpeg's `cabac_init_state`.
        // - Dependent slice segment → restore the contexts as they were at
        //   the end of the previous slice segment (after its last CTB's
        //   `end_of_slice_flag = 1` terminate bin), matching FFmpeg's
        //   behavior in `ff_hevc_cabac_init` which skips `cabac_init_state`
        //   when `dependent_slice_segment_flag == 1`. Exception: if the
        //   dependent slice's first CTB coincides with a WPP row start
        //   and the picture is wider than 1 CTB, use the "end of previous
        //   row's 2nd CTB" WPP save instead (spec 9.3.2.2 / FFmpeg
        //   `load_states` path in `ff_hevc_cabac_init`). Single-column
        //   pictures fall back to a fresh init for the row start, matching
        //   the `ctb_width == 1` branch in FFmpeg.
        //
        // The CABAC byte stream is freshly opened on the new NAL's RBSP
        // either way — the dependent slice has its own bitstream bytes,
        // only the context state is inherited.
        let pic_width_in_ctbs_for_init = sps.pic_width_in_ctbs_y();
        let wpp_for_init = pps.entropy_coding_sync_enabled_flag;
        let tiles_for_init = pps.tiles_enabled_flag;
        // slice_segment_address is a CTB address in raster scan (spec
        // 7.4.7.1); CTBs are decoded in tile scan, so convert it.
        let slice_start_ts = *tile_tables
            .ctb_addr_rs_to_ts
            .get(sh.slice_segment_address as usize)
            .ok_or(DecodeError::InvalidSyntax(
                "slice_segment_address out of range",
            ))?;
        if sh.dependent_slice_segment_flag && slice_start_ts == 0 {
            // A dependent segment continues a slice, so it cannot start the
            // picture (FFmpeg: "Impossible slice segment").
            return Err(DecodeError::InvalidSyntax(
                "dependent slice segment at the start of the picture",
            ));
        }
        let tile_of_ts = |ts: u32| {
            tile_tables
                .ctb_addr_ts_to_rs
                .get(ts as usize)
                .and_then(|&rs| tile_tables.tile_id.get(rs as usize))
                .copied()
        };
        let slice_starts_tile = tiles_for_init
            && (slice_start_ts == 0
                || tile_of_ts(slice_start_ts) != tile_of_ts(slice_start_ts - 1));
        // First CTB column of the tile holding each picture column (0
        // everywhere without tiles). With WPP, a CTB row of a tile is a
        // substream and synchronises from the row above (spec 9.3.1).
        let tile_col_start: Vec<u32> = {
            let w = pic_width_in_ctbs_for_init as usize;
            let mut starts = vec![0u32; w];
            for c in 1..w {
                starts[c] = if tile_tables.tile_id[c] != tile_tables.tile_id[c - 1] {
                    c as u32
                } else {
                    starts[c - 1]
                };
            }
            starts
        };
        let cabac_byte_offset = sh.header_size_bits / 8;
        // Checked here, before a new picture starts, as the CTB decode
        // opens its own reader.
        CabacReader::new(&nal.rbsp, cabac_byte_offset)?;

        // Spec 7.4.7.1: `entry_point_offset_minus1[i]` values are in NAL-unit
        // byte-space (they count the emulation-prevention bytes 0x03 present
        // in the raw NAL). To index into our RBSP (where EPBs have been
        // stripped), we subtract the EPBs that fall inside each NAL-space
        // target. Pre-compute the NAL-space position of the slice_segment_data
        // start — this is the RBSP cabac_byte_offset plus the count of EPBs
        // that fall inside the slice header.
        let nal_slice_data_start = {
            let mut nss = cabac_byte_offset as u32;
            for &p in &nal.epb_positions {
                if p < nss {
                    nss += 1;
                } else {
                    break;
                }
            }
            nss
        };
        let pic_width_in_ctbs = sps.pic_width_in_ctbs_y();
        let pic_height_in_ctbs = sps.pic_height_in_ctbs_y();
        let total_ctbs = pic_width_in_ctbs * pic_height_in_ctbs;

        // Phase 3c-1: a first slice segment starts a new picture. Subsequent
        // slice segments (`first_slice_segment_in_pic_flag = 0`) attach to
        // the already-in-flight picture. `pic.ctbs_decoded` counts CTBs in
        // tile-scan order (`slice_start_ts` above is the tile-scan position
        // of the raster `slice_segment_address`).
        if sh.first_slice_segment_in_pic_flag {
            // POCs are unique within a coded video sequence (spec 8.3.1):
            // a reference picture with this POC makes every reference by
            // POC ambiguous (FFmpeg: "Duplicate POC in a sequence"). A
            // picture starting a sequence unmarks them all below.
            if !irap_no_rasl_output
                && self
                    .dpb
                    .pictures()
                    .iter()
                    .any(|p| p.poc == sh.poc && p.is_reference())
            {
                return Err(DecodeError::InvalidSyntax(
                    "duplicate POC in a coded video sequence",
                ));
            }
            // Phase 3d-6: build the reference picture lists at picture start
            // (before the CTB loop) so that motion compensation can access
            // the reference frame pixel data. Previously these were built at
            // picture completion (too late for MC).
            // Reference picture set and marking (spec 8.3.2) once per
            // picture, before decoding it, whatever the first slice's type:
            // an intra first slice can be followed by P/B slices, and an
            // intra picture's RPS still decides which pictures stay
            // referenced. Lists are built per slice below.
            {
                let sps_for_rps = self.sps.as_ref().expect("sps present");
                let sps_st_rps = sps_for_rps.st_ref_pic_sets.clone();
                let log2_max_poc_lsb = sps_for_rps.log2_max_pic_order_cnt_lsb;
                let mut rps =
                    Self::derive_rps_from_slice_header_parts(&sh, &sps_st_rps, log2_max_poc_lsb)?;
                if nut.is_irap() {
                    // An IRAP picture predicts from nothing: its Curr sets
                    // are empty (8.3.2), so no reference is generated for
                    // them.
                    rps.st_curr_before.clear();
                    rps.st_curr_after.clear();
                    rps.lt_curr.clear();
                }
                if irap_no_rasl_output {
                    // 8.3.2: a picture starting a coded video sequence marks
                    // every picture in the DPB unused; its RPS names only
                    // pictures that will never be decoded (8.3.3), not
                    // older pictures that happen to share a POC.
                    self.dpb.unmark_all_references();
                } else {
                    Self::apply_rps_marking(&mut rps, &self.dpb, log2_max_poc_lsb);
                }
                self.current_rps = rps;

                // Output process before decoding the picture (C.5.2.2).
                if self.display_order {
                    if starts_new_sequence {
                        // A CRA here follows an EOS: NoOutputOfPriorPicsFlag
                        // is 1 whatever the slice header says.
                        let no_output = nut.is_cra() || sh.no_output_of_prior_pics_flag;
                        self.output_queue.start_new_sequence(no_output);
                    } else {
                        let limits = crate::output::OutputLimits::from_sps(sps_for_rps);
                        let refs: Vec<i32> = self
                            .dpb
                            .pictures()
                            .iter()
                            .filter(|p| p.is_reference())
                            .map(|p| p.poc)
                            .collect();
                        self.output_queue
                            .before_picture(&limits, refs.len(), |poc| refs.contains(&poc));
                    }
                }
            }
            Self::generate_missing_refs(
                &self.current_rps,
                &mut self.dpb,
                &mut self.zscan_cache,
                sps,
            );

            // Populate `tab_tile_id` on the fresh picture state so intra
            // availability checks can see it.
            let zscan = zscan_table(&mut self.zscan_cache, sps);
            let mut ps_enum = self.pool.new_state(sps, zscan);
            with_picture_state!(&mut ps_enum, |ps| {
                let n = ps.tab_tile_id.len();
                ps.tab_tile_id.copy_from_slice(&tile_tables.tile_id[..n]);
                ps.filter_across_tiles = pps.loop_filter_across_tiles_enabled_flag;
                // QP-prediction state (spec 8.6.1 / FFmpeg hevcdec.c:3066-3069):
                // at the first independent segment of a picture, seed `qpy_pred`
                // and `last_qp_y` from `slice_qp` and arm `first_qp_group` so the
                // first QP group predicts from `slice_qp` (fallback when left /
                // above neighbors are unavailable). When `cu_qp_delta_enabled_flag
                // = 0` nothing else updates `last_qp_y`, so it correctly stays at
                // `slice_qp` for every CU.
                ps.qpy_pred = sh.slice_qp_y;
                ps.last_qp_y = sh.slice_qp_y;
                ps.first_qp_group = !sh.dependent_slice_segment_flag;
            });
            let side = if self.threads > 1 {
                let id = self.next_picture_id;
                self.next_picture_id += 1;
                // `threads` pictures in flight plus the one receiving slices.
                let workers = self.workers.get_or_insert_with(|| {
                    crate::parallel::ThreadPool::new(self.threads + 1, "deadlib HEVC picture")
                });
                PictureSide::Thread(Box::new(start_picture_thread(
                    workers,
                    &mut self.dpb,
                    &mut self.pool,
                    id,
                    self.threads,
                    ps_enum,
                    sh.poc,
                    (sps, pps),
                    self.in_flight.is_empty(),
                )))
            } else {
                PictureSide::Local(Box::new(PictureDecode {
                    state: ps_enum,
                    saved_cabac_state: None,
                    saved_wpp_cabac_state: None,
                    rows_in_parallel: false,
                }))
            };
            self.current_picture = Some(PictureInProgress {
                side,
                last_slice_header: sh.clone(),
                last_independent_slice_header: sh.clone(),
                total_ctbs,
                slice_ref_pocs: Vec::new(),
                nal_unit_type: nut,
            });
        } else {
            let pic = self
                .current_picture
                .as_ref()
                .ok_or(DecodeError::InvalidSyntax(
                    "non-first slice segment without an active picture",
                ))?;
            if pic.total_ctbs != total_ctbs {
                return Err(DecodeError::InvalidSyntax(
                    "slice SPS dimensions changed within picture",
                ));
            }
            // Allow non-contiguous slice segment addresses — the CTB loop
            // starts at `sh.slice_segment_address` regardless. This handles
            // streams where slices don't arrive in strict tile-scan order
            // or where there are gaps between slice segments.
        }

        // Reference picture lists are per slice (spec 8.3.4): every
        // independent slice segment builds its own from its header; a
        // dependent segment keeps its parent's.
        if !sh.dependent_slice_segment_flag {
            self.current_ref_list_l0.clear();
            self.current_ref_list_l1.clear();
            if sh.slice_type != SliceType::I {
                // With frame threading the picture is in the DPB already.
                let current = match self.current_picture.as_ref().map(|p| &p.side) {
                    Some(PictureSide::Thread(t)) => Some(&t.picture),
                    _ => None,
                };
                let (l0, l1) =
                    Self::build_ref_pic_lists(&self.current_rps, &self.dpb, &sh, current)?;
                // The DPB is cleared when the picture geometry changes, and
                // missing references are generated with the current SPS.
                debug_assert!(l0.iter().chain(&l1).all(|p| {
                    (p.width, p.height, p.log2_ctb_size)
                        == (
                            sps.pic_width_in_luma_samples,
                            sps.pic_height_in_luma_samples,
                            sps.ctb_log2_size_y,
                        )
                }));
                self.current_ref_list_l0 = l0;
                self.current_ref_list_l1 = l1;
            }
            let entry = (
                sh.slice_segment_address as i32,
                Self::slice_ref_lists(&self.current_ref_list_l0, &self.current_ref_list_l1),
            );
            if let Some(pic) = self.current_picture.as_mut() {
                pic.slice_ref_pocs.push(entry);
            }
        }
        self.slice_initialized = true;

        // Borrow the in-flight picture mutably for the rest of decode.
        let pic = self
            .current_picture
            .as_mut()
            .expect("current_picture set above");

        // QP-prediction state per slice segment (spec 8.6.1 / FFmpeg
        // hevcdec.c:3066-3069). `first_qp_group` resets at every segment
        // start — to true for independent segments (fallback to slice_qp),
        // to false for dependent ones (inherit parent's `qpy_pred`).
        // `last_qp_y` (FFmpeg: `lc->qp_y`) must be re-seeded to
        // `slice_qp_y` at the start of every independent slice segment
        // (FFmpeg `hls_decode_entry`: `lc->qp_y = s->sh.slice_qp_y`).
        // A dependent segment that starts a tile, or a CTB row under WPP,
        // also starts a new QP prediction chain: the first quantization group
        // of a tile / WPP row predicts from SliceQpY (spec 8.6.1).
        let segment_starts_qp_chain = !sh.dependent_slice_segment_flag
            || slice_starts_tile
            || (wpp_for_init
                && sh
                    .slice_segment_address
                    .is_multiple_of(pic_width_in_ctbs_for_init));

        // Phase 3c-3 (WPP): saved CABAC context state captured after the
        // second CTB of each row, to be loaded at the start of the next row.
        // Not used by tiles — tiles reinit from the slice QP at every tile
        // boundary instead. Phase 3c-4: for dependent slices we seed this
        // from the picture-level `saved_wpp_cabac_state` so that the
        // first inner-row reinit inside the dependent slice can still find
        // a state from the previous slice's row.

        // Phase 3d-3/3d-5: construct SliceParams for the CU tree.
        // Include POC information so the AMVP candidate list builder can
        // check same-ref and compute MV scaling.
        let ref_list_l0_pocs: Vec<i32> = self.current_ref_list_l0.iter().map(|p| p.poc).collect();
        let ref_list_l1_pocs: Vec<i32> = self.current_ref_list_l1.iter().map(|p| p.poc).collect();
        // Phase 3e: derive the collocated reference picture for temporal MVP.
        let collocated_ref = if sh.slice_temporal_mvp_enabled_flag && sh.slice_type != SliceType::I
        {
            let col_list = if sh.collocated_from_l0_flag {
                &self.current_ref_list_l0
            } else {
                &self.current_ref_list_l1
            };
            let col_idx = sh.collocated_ref_idx as usize;
            col_list.get(col_idx).cloned()
        } else {
            None
        };
        let slice_params = crate::cu_tree::SliceParams {
            slice_type: sh.slice_type,
            max_num_merge_cand: sh.max_num_merge_cand,
            num_ref_idx_l0_active: sh.num_ref_idx_l0_active_minus1 + 1,
            num_ref_idx_l1_active: if sh.slice_type == SliceType::B {
                sh.num_ref_idx_l1_active_minus1 + 1
            } else {
                0
            },
            mvd_l1_zero_flag: sh.mvd_l1_zero_flag,
            log2_parallel_merge_level: (pps.log2_parallel_merge_level_minus2 + 2) as u8,
            poc: sh.poc,
            ref_pic_list_pocs: [ref_list_l0_pocs, ref_list_l1_pocs],
            ref_pic_list_is_lt: Self::slice_ref_lists(
                &self.current_ref_list_l0,
                &self.current_ref_list_l1,
            )
            .is_long_term,
            picture_slice_ref_lists: pic.slice_ref_pocs.clone(),
            ref_frames_l0: self.current_ref_list_l0.clone(),
            ref_frames_l1: self.current_ref_list_l1.clone(),
            collocated_ref,
            slice_temporal_mvp_enabled_flag: sh.slice_temporal_mvp_enabled_flag,
            collocated_from_l0_flag: sh.collocated_from_l0_flag,
            deblocking_disabled: sh.slice_deblocking_filter_disabled_flag,
            slice_cb_qp_offset: sh.slice_cb_qp_offset,
            slice_cr_qp_offset: sh.slice_cr_qp_offset,
            cu_chroma_qp_offset_enabled_flag: sh.cu_chroma_qp_offset_enabled_flag,
            weighted_pred_flag: (pps.weighted_pred_flag && sh.slice_type == SliceType::P)
                || (pps.weighted_bipred_flag && sh.slice_type == SliceType::B),
            pred_weight_table: sh.pred_weight_table.clone(),
        };

        // Pre-compute the recorded_slice_addr for use in the CTB loop.
        let recorded_slice_addr = if sh.dependent_slice_segment_flag {
            pic.last_independent_slice_header.slice_segment_address as i32
        } else {
            sh.slice_segment_address as i32
        };

        // The CTB loop, deblock, SAO, and DPB insertion all operate on a
        // generic PictureState<P>. Dispatch once via the enum variant and
        // execute the entire inner body monomorphized for the right P.

        let job = SliceJob {
            rbsp: nal.rbsp.to_vec(),
            epb_positions: nal.epb_positions.clone(),
            sps: Arc::clone(sps),
            pps: Arc::clone(pps),
            tile_tables: Arc::clone(tile_tables),
            slice_params,
            recorded_slice_addr,
            slice_start_ts,
            slice_starts_tile,
            tile_col_start,
            segment_starts_qp_chain,
            cabac_byte_offset,
            nal_slice_data_start,
            sh: sh.clone(),
        };
        // The picture's latest slice header, and the one dependent segments
        // inherit from, whether or not this segment's data decodes: with
        // frame threading the data decodes later, on the picture's thread.
        if !sh.dependent_slice_segment_flag {
            pic.last_independent_slice_header = sh.clone();
        }
        pic.last_slice_header = sh;
        // The picture ends when the next one starts (or at flush), with any
        // thread count: a picture thread cannot tell the calling thread in
        // time that its CTBs are complete, and a slice segment arriving
        // after that is then handled the same way whatever the thread count
        // (it may not rewrite complete CTB rows).
        match &mut pic.side {
            PictureSide::Local(decode) => decode.decode_slice(&job, self.threads)?,
            PictureSide::Thread(t) => {
                // A send fails only if the thread panicked, which joining
                // it reports.
                let _ = t.jobs.send(PictureMessage::Slice(Box::new(job)));
            }
        }
        Ok(None)
    }

    /// Spec 8.3.1: prevTid0Pic is simply the previous TemporalId-0 picture
    /// that is not RASL/RADL/sub-layer non-reference. IRAP pictures qualify
    /// like any other, so store their actual POC (a mid-stream CRA keeps a
    /// non-zero POC; a BLA's POC is its LSB).
    fn update_prev_tid0(&mut self, last_sh: &SliceHeader) {
        let is_tid0 = last_sh.temporal_id == 0
            && !matches!(
                last_sh.nal_unit_type,
                NalUnitType::TrailN
                    | NalUnitType::TsaN
                    | NalUnitType::StsaN
                    | NalUnitType::RadlN
                    | NalUnitType::RadlR
                    | NalUnitType::RaslN
                    | NalUnitType::RaslR
            );
        if is_tid0 {
            self.prev_tid0_poc = last_sh.poc;
        }
    }

    /// The calling thread's end of a picture decoding on its own thread:
    /// hand it the finish, and do now what a sequential decode does after
    /// the picture (prevTid0Pic, the output process, with its frame
    /// pending until the thread delivers it). The picture is in the DPB
    /// already.
    fn finish_threaded_picture(
        &mut self,
        last_sh: SliceHeader,
        slice_ref_pocs: Vec<(i32, crate::dpb::SliceRefLists)>,
        thread: PictureThread,
    ) -> Result<Option<Frame>, DecodeError> {
        let sps = self
            .sps
            .clone()
            .ok_or(DecodeError::InvalidSyntax("picture without active SPS"))?;
        let pps = self
            .pps
            .clone()
            .ok_or(DecodeError::InvalidSyntax("picture without active PPS"))?;
        let _ = thread.picture.slice_ref_pic_list_pocs.set(slice_ref_pocs);
        let limits = crate::output::OutputLimits::from_sps(&sps);
        let _ = thread.jobs.send(PictureMessage::Finish {
            sps,
            pps,
            last_sh: Box::new(last_sh.clone()),
        });
        self.update_prev_tid0(&last_sh);
        let output = last_sh.pic_output_flag;
        if self.display_order {
            if output {
                self.output_queue
                    .after_picture_pending(&limits, thread.id, last_sh.poc);
            } else {
                self.output_queue.after_picture(&limits, None);
            }
        }
        self.in_flight.push_back(InFlight {
            id: thread.id,
            handle: thread.handle,
            output,
        });
        // At most `threads` pictures in flight: wait for the oldest.
        while self.in_flight.len() > self.threads {
            self.collect_picture();
        }
        Ok(None)
    }

    /// Join the oldest in-flight picture (waiting for it) and deliver its
    /// frame and errors.
    fn collect_picture(&mut self) {
        let Some(f) = self.in_flight.pop_front() else {
            return;
        };
        let result = match f.handle.join() {
            Ok(r) => r,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        self.pool.recycle_state(result.state);
        self.deferred_errors.extend(result.errors);
        let frame = match result.frame {
            Ok(frame) => Some(frame),
            Err(e) => {
                self.deferred_errors.push_back(e);
                None
            }
        };
        if !f.output {
            return;
        }
        if self.display_order {
            self.output_queue.resolve(f.id, frame);
        } else if let Some(frame) = frame {
            self.pending_frames.push_back(frame);
        }
    }

    /// Collect the in-flight pictures that have finished, oldest first
    /// (stopping at the first still decoding, to keep decoding order), or
    /// all of them when `wait` is set.
    fn collect_pictures(&mut self, wait: bool) {
        while let Some(f) = self.in_flight.front_mut() {
            if !wait && !f.handle.is_finished() {
                break;
            }
            self.collect_picture();
        }
    }

    /// Finish a picture: fill CTBs that were never decoded (slices lost or
    /// failed), run the in-loop filters, store it in the DPB and hand the
    /// frame to the output (decoding-order return value or display-order
    /// queue).
    fn finish_picture(&mut self, mut pic: PictureInProgress) -> Result<Option<Frame>, DecodeError> {
        let mut decode = match pic.side {
            PictureSide::Local(decode) => decode,
            PictureSide::Thread(thread) => {
                return self.finish_threaded_picture(
                    pic.last_slice_header,
                    pic.slice_ref_pocs,
                    *thread,
                );
            }
        };
        let threads = self.threads.max(1);
        let sps = self
            .sps
            .as_ref()
            .ok_or(DecodeError::InvalidSyntax("picture without active SPS"))?;
        let pps = self
            .pps
            .as_ref()
            .ok_or(DecodeError::InvalidSyntax("picture without active PPS"))?;
        let last_sh = &pic.last_slice_header;
        let emitted_frame = decode.finish(sps, pps, last_sh, threads)?;
        let last_sh_cloned = last_sh.clone();
        let picture_poc = last_sh_cloned.poc;
        let pic_width = sps.pic_width_in_luma_samples;
        let pic_height = sps.pic_height_in_luma_samples;

        // The borrows `sps` / `pps` / `tile_tables` / `last_sh` are no
        // longer used beyond this point — NLL will release them here so
        // we can take mutable borrows of `self.*` below.

        self.update_prev_tid0(&last_sh_cloned);

        // Insert the fresh picture into the DPB.
        let ref_list_l0_pocs: Vec<i32> = self.current_ref_list_l0.iter().map(|p| p.poc).collect();
        let ref_list_l1_pocs: Vec<i32> = self.current_ref_list_l1.iter().map(|p| p.poc).collect();

        let decoded_pic = with_picture_state!(&mut decode.state, |state| {
            // The planes keep the CTU-aligned stride and chroma geometry of
            // the picture state; motion compensation reads them through
            // these fields rather than assuming `stride == width`.
            let (y_stride, uv_stride, chroma_shift_w, chroma_shift_h) = (
                state.y_stride,
                state.uv_stride,
                state.chroma_shift_w,
                state.chroma_shift_h,
            );
            let (y, u, v, tab_mvf, log2_min_pu_size, min_pu_width, log2_ctb_size) =
                state.take_planes_and_mvf();
            let mut dp = DecodedPicture::new_with_mvf(
                y,
                u,
                v,
                pic_width,
                pic_height,
                picture_poc,
                tab_mvf,
                log2_min_pu_size,
                min_pu_width,
                log2_ctb_size,
                [ref_list_l0_pocs, ref_list_l1_pocs],
            );
            dp.y_stride = y_stride;
            dp.uv_stride = uv_stride;
            dp.chroma_shift_w = chroma_shift_w;
            dp.chroma_shift_h = chroma_shift_h;
            let _ = dp
                .slice_ref_pic_list_pocs
                .set(std::mem::take(&mut pic.slice_ref_pocs));
            dp.ctb_slice_addr = std::mem::take(&mut state.tab_slice_addr_rs);
            Arc::new(dp)
        });
        self.pool.recycle_state(decode.state);
        decoded_pic.mark(PictureReferenceStatus::ShortTerm);
        decoded_pic.set_output();
        self.dpb.insert(decoded_pic);
        for retired in self.dpb.cleanup_unused() {
            self.pool.recycle_picture(retired);
        }

        // PicOutputFlag (spec 8.1.3): a picture with pic_output_flag = 0 is
        // decoded and kept for reference but never output. (Skipped RASL
        // pictures never reach this point.)
        let frame = last_sh_cloned.pic_output_flag.then_some(emitted_frame);
        if self.display_order {
            // Output process once the picture is decoded (C.5.2.3).
            let sps = self
                .sps
                .as_ref()
                .expect("sps present for a decoded picture");
            let limits = crate::output::OutputLimits::from_sps(sps);
            self.output_queue.after_picture(&limits, frame);
            Ok(None)
        } else {
            Ok(frame)
        }
    }

    /// Spec 8.3.3: generate the reference pictures the current picture uses
    /// (RefPicSetStCurrBefore, StCurrAfter, LtCurr) that are not in the
    /// DPB. In a conformant stream this only happens after an error: the
    /// reference was lost or failed to decode. The stand-in is mid-grey with
    /// no motion, so the current picture still decodes, with errors where
    /// it predicts from the missing picture.
    fn generate_missing_refs(
        rps: &ReferencePictureSets,
        dpb: &mut DecodedPictureBuffer,
        zscan_cache: &mut Option<(ZscanKey, Arc<Vec<i32>>)>,
        sps: &Sps,
    ) {
        let mut missing: Vec<(i32, PictureReferenceStatus)> = Vec::new();
        for &poc in rps.st_curr_before.iter().chain(&rps.st_curr_after) {
            if dpb.find_st_ref(poc).is_none() {
                missing.push((poc, PictureReferenceStatus::ShortTerm));
            }
        }
        for &poc in &rps.lt_curr {
            let present = dpb
                .pictures()
                .iter()
                .any(|p| p.poc == poc && p.reference_status() == PictureReferenceStatus::LongTerm);
            if !present {
                missing.push((poc, PictureReferenceStatus::LongTerm));
            }
        }
        // A POC named twice (a damaged RPS) is generated once.
        missing.sort_unstable_by_key(|&(poc, _)| poc);
        missing.dedup_by_key(|&mut (poc, _)| poc);
        for (poc, status) in missing {
            let zscan = zscan_table(zscan_cache, sps);
            let mut state = new_picture_state(sps, zscan);
            let pic = with_picture_state!(&mut state, |st| generated_reference(st, poc));
            pic.mark(status);
            // Never output.
            pic.set_output();
            dpb.insert(pic);
        }
        // At most 16 references plus 15 stand-ins, and the pictures not yet
        // output or still decoding on their threads.
        debug_assert!(dpb.len() <= 64, "DPB holds {} pictures", dpb.len());
    }

    /// Phase 3d-1: compute a picture's POC from `prev_tid0_poc`, the
    /// slice header's `pic_order_cnt_lsb`, and the SPS's
    /// `log2_max_pic_order_cnt_lsb`. Follows HEVC spec 8.3.1 + FFmpeg's
    /// `ff_hevc_compute_poc2` in `ps.c`.
    ///
    /// For BLA pictures the POC MSB is explicitly reset to 0, matching
    /// FFmpeg's behavior.
    fn compute_poc(
        prev_tid0_poc: i32,
        slice_pic_order_cnt_lsb: u32,
        log2_max_pic_order_cnt_lsb: u8,
        irap_no_rasl_output: bool,
    ) -> Option<i32> {
        let max_poc_lsb = 1i32 << log2_max_pic_order_cnt_lsb;
        let prev_poc_lsb = prev_tid0_poc.rem_euclid(max_poc_lsb);
        let prev_poc_msb = prev_tid0_poc - prev_poc_lsb;
        let cur_lsb = slice_pic_order_cnt_lsb as i32;
        let mut poc_msb = if cur_lsb < prev_poc_lsb && (prev_poc_lsb - cur_lsb) >= max_poc_lsb / 2 {
            prev_poc_msb.checked_add(max_poc_lsb)?
        } else if cur_lsb > prev_poc_lsb && (cur_lsb - prev_poc_lsb) > max_poc_lsb / 2 {
            prev_poc_msb.checked_sub(max_poc_lsb)?
        } else {
            prev_poc_msb
        };
        // Spec 8.3.1: an IRAP picture with NoRaslOutputFlag = 1 (BLA, or a
        // CRA that starts a coded video sequence) has POC MSB = 0.
        if irap_no_rasl_output {
            poc_msb = 0;
        }
        // PicOrderCntVal is in the i32 range (8.3.1).
        poc_msb.checked_add(cur_lsb)
    }

    /// Phase 3d-1: derive the reference picture sets for the current
    /// picture from the slice header and the (cloned) SPS ST-RPS array.
    /// This follows spec 8.3.2 but simplified — we don't yet track the
    /// "follow" sets precisely or complain about missing refs, since no
    /// actual decoding uses them yet. Good enough to have the POC lists
    /// plumbed so Phase 3d-2 can pick them up.
    ///
    /// Takes an owned slice of SPS ST-RPSs rather than `&Sps` so the
    /// caller can drop its immutable borrow of `self.sps` and mutably
    /// touch `self.dpb` in the surrounding code. A real
    /// spec-parity derivation would also consume the DPB to walk the
    /// existing picture pool, but Phase 3d-1 only needs POC lists.
    fn derive_rps_from_slice_header_parts(
        sh: &SliceHeader,
        sps_st_ref_pic_sets: &[ShortTermRps],
        log2_max_pic_order_cnt_lsb: u8,
    ) -> Result<ReferencePictureSets, DecodeError> {
        let mut rps = ReferencePictureSets::default();

        // IDR pictures never reference anything from the DPB.
        if sh.nal_unit_type.is_idr() {
            return Ok(rps);
        }
        // Reference POCs are in the i32 range too (8.3.1; FFmpeg rejects a
        // long-term POC beyond it).
        let out_of_range = || DecodeError::InvalidSyntax("reference POC out of range");

        // Resolve the active short-term RPS.
        let st_ref: Option<&ShortTermRps> = if sh.short_term_ref_pic_set_sps_flag {
            sps_st_ref_pic_sets.get(sh.short_term_ref_pic_set_idx as usize)
        } else {
            sh.short_term_rps.as_ref()
        };

        if let Some(st) = st_ref {
            // Short-term "before current" (negative deltas the current
            // picture uses).
            for (i, delta) in st.delta_poc_s0.iter().enumerate() {
                let ref_poc = sh.poc.checked_add(*delta).ok_or_else(out_of_range)?;
                if st.used_by_curr_pic_s0_flag.get(i).copied().unwrap_or(false) {
                    rps.st_curr_before.push(ref_poc);
                } else {
                    rps.st_foll.push(ref_poc);
                }
            }
            // Short-term "after current" (positive deltas the current
            // picture uses).
            for (i, delta) in st.delta_poc_s1.iter().enumerate() {
                let ref_poc = sh.poc.checked_add(*delta).ok_or_else(out_of_range)?;
                if st.used_by_curr_pic_s1_flag.get(i).copied().unwrap_or(false) {
                    rps.st_curr_after.push(ref_poc);
                } else {
                    rps.st_foll.push(ref_poc);
                }
            }
        }

        // Long-term references per spec 8.3.2 / FFmpeg decode_lt_rps.
        // When delta_poc_msb_present_flag is true, compute full POC:
        //   PocLt = poc_lsb + cur_poc - delta * MaxPicOrderCntLsb - cur_poc_lsb
        // When false, store just the POC LSB for LSB-based DPB matching.
        let max_poc_lsb = 1i32 << log2_max_pic_order_cnt_lsb;
        let cur_poc = sh.poc;
        let cur_poc_lsb = sh.slice_pic_order_cnt_lsb as i32;
        let num_lt_sps = sh.long_term_rps.num_long_term_sps;
        let mut prev_delta_msb: i64 = 0;

        for (i, poc_lsb) in sh.long_term_rps.poc_lsb_lt.iter().enumerate() {
            let msb_present = sh
                .long_term_rps
                .delta_poc_msb_present_flag
                .get(i)
                .copied()
                .unwrap_or(false);

            let ref_poc = if msb_present {
                // DeltaPocMsbCycleLt (spec 7.4.7.1 eq. 7-52) accumulates
                // within the SPS entries and within the slice-coded ones,
                // restarting at the first of each. As HM and FFmpeg
                // (`if (i && i != nb_sps) delta += prev_delta_msb;`), only
                // entries with an MSB carry the sum.
                let raw_delta = sh
                    .long_term_rps
                    .delta_poc_msb_cycle_lt
                    .get(i)
                    .copied()
                    .unwrap_or(0) as i64;
                let delta = if i > 0 && i != num_lt_sps {
                    raw_delta + prev_delta_msb
                } else {
                    raw_delta
                };
                prev_delta_msb = delta;
                // FFmpeg: poc = rps->poc[i] + cur_poc - delta * max_poc_lsb - poc_lsb
                i32::try_from(
                    *poc_lsb as i64 + cur_poc as i64
                        - delta * max_poc_lsb as i64
                        - cur_poc_lsb as i64,
                )
                .map_err(|_| out_of_range())?
            } else {
                *poc_lsb as i32
            };

            let used = sh
                .long_term_rps
                .used_by_curr_pic_lt_flag
                .get(i)
                .copied()
                .unwrap_or(false);
            if used {
                rps.lt_curr.push(ref_poc);
            } else {
                rps.lt_foll.push(ref_poc);
            }
            rps.lt_poc_msb_present.push(msb_present);
        }

        Ok(rps)
    }

    /// Phase 3d-2: build `RefPicList0` / `RefPicList1` for a P or B slice
    /// per HEVC spec 8.3.2. Caller must ensure the slice is P or B (I
    /// slices get empty lists).
    ///
    /// Returns `(L0, L1)` where L1 is empty for P slices. Each list has
    /// exactly `num_ref_idx_l{0,1}_active_minus1 + 1` entries and every
    /// entry points to a picture already in the DPB.
    #[allow(clippy::type_complexity)]
    /// Activate the PPS named by a picture's first slice header, and the SPS
    /// it refers to (spec 7.4.2.4.2). Only does work when the IDs changed or
    /// a parameter set arrived since the last activation.
    fn activate_parameter_sets(
        &mut self,
        nal: &NalUnit<'_>,
        nut: NalUnitType,
    ) -> Result<(), DecodeError> {
        // Slice header prefix: first_slice_segment_in_pic_flag,
        // [no_output_of_prior_pics_flag for IRAP], slice_pic_parameter_set_id.
        let mut r = crate::bitstream::BitstreamReader::new(&nal.rbsp);
        r.read_bit()?;
        if nut.is_irap() {
            r.read_bit()?;
        }
        let pps_id = r.read_ue()?;
        if !self.parameter_sets_changed && self.active_pps_id == Some(pps_id) {
            return Ok(());
        }
        let pps = match self.pps_table.get(pps_id as usize).and_then(Option::as_ref) {
            Some(Ok(pps)) => pps.clone(),
            Some(Err(e)) => return Err(e.clone()),
            None => return Err(DecodeError::InvalidSyntax("slice refers to a missing PPS")),
        };
        let sps_id = pps.pps_seq_parameter_set_id;
        let sps = match self.sps_table.get(sps_id as usize).and_then(Option::as_ref) {
            Some(Ok(sps)) => sps.clone(),
            Some(Err(e)) => return Err(e.clone()),
            None => return Err(DecodeError::InvalidSyntax("PPS refers to a missing SPS")),
        };
        // A change of picture format or geometry invalidates every picture
        // in the DPB (as FFmpeg clears its references on an SPS change; in
        // a conforming stream a new SPS starts a CVS anyway). References
        // must share the sample type and chroma geometry of the pictures
        // predicted from them (motion compensation reads them with the
        // current picture's chroma subsampling), and temporal MV
        // prediction indexes the collocated motion field and slice map
        // with the current picture's coordinates and CTB size.
        let format = |s: &Sps| {
            (
                (s.bit_depth_luma, s.bit_depth_chroma, s.chroma_format_idc),
                (s.pic_width_in_luma_samples, s.pic_height_in_luma_samples),
                (s.ctb_log2_size_y, s.min_cb_log2_size_y),
            )
        };
        let old_format = self.sps.as_deref().map(format);
        if old_format.is_some() && old_format != Some(format(&sps)) {
            self.dpb.clear();
            self.current_picture = None;
        }
        self.sps = Some(Arc::new(sps));
        // A fresh PPS copy has no tile geometry resolved yet; it is derived
        // for this SPS at the next slice, and the scan tables rebuilt.
        self.pps = Some(Arc::new(pps));
        self.tile_tables = None;
        self.active_sps_id = Some(sps_id);
        self.active_pps_id = Some(pps_id);
        self.parameter_sets_changed = false;
        Ok(())
    }

    /// POCs and long-term flags of the current slice's reference lists.
    fn slice_ref_lists(
        l0: &[Arc<DecodedPicture>],
        l1: &[Arc<DecodedPicture>],
    ) -> crate::dpb::SliceRefLists {
        let pocs = |l: &[Arc<DecodedPicture>]| l.iter().map(|p| p.poc).collect::<Vec<i32>>();
        let lt = |l: &[Arc<DecodedPicture>]| {
            l.iter()
                .map(|p| p.reference_status() == PictureReferenceStatus::LongTerm)
                .collect::<Vec<bool>>()
        };
        crate::dpb::SliceRefLists {
            pocs: [pocs(l0), pocs(l1)],
            is_long_term: [lt(l0), lt(l1)],
        }
    }

    fn build_ref_pic_lists(
        rps: &ReferencePictureSets,
        dpb: &DecodedPictureBuffer,
        sh: &SliceHeader,
        current: Option<&Arc<DecodedPicture>>,
    ) -> Result<RefPicLists, DecodeError> {
        let num_poc_total_curr = rps.num_poc_total_curr();
        let active_l0 = sh.num_ref_idx_l0_active_minus1 as usize + 1;
        let num_rps_curr_temp_list0 = active_l0.max(num_poc_total_curr);
        let temp0 = build_ref_pic_list_temp0(rps, num_rps_curr_temp_list0);
        let l0_pocs = apply_ref_pic_list_modification(
            &temp0,
            sh.ref_pic_list_modification
                .ref_pic_list_modification_flag_l0,
            &sh.ref_pic_list_modification.list_entry_l0,
            active_l0,
        )?;
        let l0 = resolve_ref_pics(dpb, &l0_pocs, current)?;

        let l1 = if sh.slice_type == SliceType::B {
            let active_l1 = sh.num_ref_idx_l1_active_minus1 as usize + 1;
            let num_rps_curr_temp_list1 = active_l1.max(num_poc_total_curr);
            let temp1 = build_ref_pic_list_temp1(rps, num_rps_curr_temp_list1);
            let l1_pocs = apply_ref_pic_list_modification(
                &temp1,
                sh.ref_pic_list_modification
                    .ref_pic_list_modification_flag_l1,
                &sh.ref_pic_list_modification.list_entry_l1,
                active_l1,
            )?;
            resolve_ref_pics(dpb, &l1_pocs, current)?
        } else {
            Vec::new()
        };

        Ok((l0, l1))
    }

    /// Phase 3d-1: walk the RPS and mark each referenced picture in the DPB
    /// (spec 8.3.2). Pictures referenced by the RPS become ShortTerm or
    /// LongTerm; everything else is flipped to UnusedForReference.
    ///
    /// Iterates the RPS entries (not the DPB) so that missing-reference
    /// cases are detected. Per spec, a missing reference should generate a
    /// placeholder picture — we log a warning and skip, which is correct
    /// for conformant streams.
    pub(crate) fn apply_rps_marking(
        rps: &mut ReferencePictureSets,
        dpb: &DecodedPictureBuffer,
        log2_max_pic_order_cnt_lsb: u8,
    ) {
        let max_poc_lsb = 1i32 << log2_max_pic_order_cnt_lsb;

        // Step 1: unmark everything. All pictures start as UnusedForReference.
        dpb.unmark_all_references();

        // Step 2: mark short-term references by full POC.
        let st_pocs = rps
            .st_curr_before
            .iter()
            .chain(&rps.st_curr_after)
            .chain(&rps.st_foll);
        for &poc in st_pocs {
            if let Some(pic) = dpb.find_by_poc(poc) {
                pic.mark(PictureReferenceStatus::ShortTerm);
            }
            // Missing ST ref: conformant streams won't hit this; for
            // non-conformant streams the error surfaces later in
            // resolve_ref_pics when building the actual ref lists.
        }

        // Step 3: mark long-term references. When `poc_msb_present` is true
        // for an entry, match by full POC (the entry already contains the
        // resolved absolute POC). When false, match by POC LSB (spec 8.3.2 /
        // FFmpeg find_ref_idx: `mask = use_msb ? ~0 : (1 << log2) - 1`).
        // A matched LSB-only entry is rewritten to the picture's full POC, so
        // that reference list construction (which looks pictures up by POC)
        // resolves it.
        let n_curr = rps.lt_curr.len();
        let lt_all: Vec<i32> = rps.lt_curr.iter().chain(&rps.lt_foll).copied().collect();
        for (idx, &poc_or_lsb) in lt_all.iter().enumerate() {
            let use_msb = rps.lt_poc_msb_present.get(idx).copied().unwrap_or(false);
            for pic in dpb.pictures() {
                if pic.reference_status() == PictureReferenceStatus::ShortTerm {
                    continue; // ST takes priority
                }
                let matches = if use_msb {
                    pic.poc == poc_or_lsb
                } else {
                    pic.poc.rem_euclid(max_poc_lsb) == poc_or_lsb
                };
                if matches {
                    pic.mark(PictureReferenceStatus::LongTerm);
                    if idx < n_curr {
                        rps.lt_curr[idx] = pic.poc;
                    } else {
                        rps.lt_foll[idx - n_curr] = pic.poc;
                    }
                    break;
                }
            }
        }
    }
}
