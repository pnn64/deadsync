//! Wavefront parallel decoding (WPP) of the CTB rows of one slice.
//!
//! With `entropy_coding_sync_enabled_flag` each CTB row of a slice is its
//! own CABAC substream, starting at an entry point, whose contexts start
//! from those saved after the second CTB of the row above (spec 9.3.1).
//! Each row depends on the rows above only through CTBs up to one column to
//! its right (above-right neighbours and that context hand-off), so row
//! `r` can decode CTB `c` as soon as row `r - 1` has finished CTB `c + 1`.
//!
//! [`decode_rows`] hands the rows of a slice to worker threads (row `r0 + k`
//! to worker `k mod n`). Each worker decodes into a `PictureState` whose
//! tables are views of the picture's (`PictureState::worker_view`) and
//! publishes its progress per CTB in a shared `RowProgress`; a row waits
//! for the row above before each CTB. `crate::shared` documents the
//! protocol that makes the shared tables sound.
//!
//! The result is exactly that of sequential decoding, including for broken
//! streams: when a row fails or the slice ends early, the rows below it,
//! which a sequential decoder would never have reached, are discarded and
//! their tables reset (`PictureState::reset_ctbs`).

use std::sync::{Arc, Mutex};

use crate::cabac::{CabacContexts, CabacReader};
use crate::cu_tree::{PictureState, SliceParams, decode_coding_quadtree};
use crate::error::DecodeError;
use crate::pixel::Pixel;
use crate::pps::Pps;
use crate::shared::RowProgress;
use crate::slice::SliceHeader;
use crate::sps::Sps;

/// Everything about one slice the CTB decode needs, shared by all rows.
pub(crate) struct SliceContext<'a> {
    pub sps: &'a Sps,
    pub pps: &'a Pps,
    pub sh: &'a SliceHeader,
    pub slice_params: &'a SliceParams,
    /// The slice address CTBs of this slice record (that of its
    /// independent slice segment).
    pub recorded_slice_addr: i32,
}

/// Decode CTB `ctb_addr_rs` (tile-scan position `ctb_addr_ts`): record its
/// slice-level tables, then parse SAO parameters and the coding quadtree.
/// Returns whether the slice segment continues (`end_of_slice_segment_flag
/// == 0`). Used by both the sequential and the wavefront CTB loops. A CTB
/// that fails is left marked undecoded (grey-filled at the picture's end).
pub(crate) fn decode_ctb<P: Pixel>(
    cabac: &mut CabacReader,
    contexts: &mut CabacContexts,
    state: &mut PictureState<P>,
    ctx: &SliceContext,
    ctb_addr_rs: u32,
    ctb_addr_ts: u32,
) -> Result<bool, DecodeError> {
    let (sps, sh) = (ctx.sps, ctx.sh);
    let w = sps.pic_width_in_ctbs_y();
    let ctb_size = 1u32 << sps.ctb_log2_size_y;
    let (col, row) = (ctb_addr_rs % w, ctb_addr_rs / w);
    let rs = ctb_addr_rs as usize;
    state.tab_slice_addr_rs[rs] = ctx.recorded_slice_addr;
    state.tab_ctb_ts[rs] = ctb_addr_ts;
    state.tab_deblock_offsets[rs] = (
        (sh.slice_beta_offset_div2 * 2) as i8,
        (sh.slice_tc_offset_div2 * 2) as i8,
    );
    state.filter_slice_edges[rs] = sh.slice_loop_filter_across_slices_enabled_flag;
    crate::sao::decode_sao_param(
        cabac,
        contexts,
        state,
        sps,
        ctx.pps,
        sh,
        col as usize,
        row as usize,
    );
    let result = decode_coding_quadtree(
        cabac,
        contexts,
        state,
        sps,
        ctx.pps,
        sh.slice_qp_y,
        ctx.slice_params,
        col * ctb_size,
        row * ctb_size,
        sps.ctb_log2_size_y,
        0,
    );
    if result.is_err() {
        // A CTB that fails part-way is treated as never decoded, so the end
        // of the picture fills it mid-grey: its samples are otherwise partly
        // whatever its (recycled) plane held before, which depends on
        // buffer reuse and so differs between thread counts.
        state.tab_slice_addr_rs[rs] = -1;
    }
    result
}

/// Where the substreams of a slice start in the RBSP.
pub(crate) struct Substreams<'a> {
    pub rbsp: &'a [u8],
    /// RBSP byte offset of the slice data (the first substream).
    pub data_start: usize,
    /// NAL-unit byte offset of the slice data (entry points count the
    /// emulation-prevention bytes, spec 7.4.7.1).
    pub nal_data_start: u32,
    pub epb_positions: &'a [u32],
    pub entry_point_offsets: &'a [u32],
}

impl Substreams<'_> {
    /// Number of substreams (entry points + 1).
    pub fn count(&self) -> usize {
        self.entry_point_offsets.len() + 1
    }

    /// RBSP byte offset where substream `i` starts.
    pub fn start(&self, i: usize) -> Result<usize, DecodeError> {
        if i == 0 {
            return Ok(self.data_start);
        }
        let nal_offset = *self
            .entry_point_offsets
            .get(i - 1)
            .ok_or(DecodeError::InvalidSyntax(
                "slice missing entry_point_offset for substream",
            ))?;
        // Saturating: a damaged offset fails in reinit_at.
        let nal_target = self.nal_data_start.saturating_add(nal_offset);
        let epbs = self
            .epb_positions
            .iter()
            .filter(|&&p| p >= self.nal_data_start && p < nal_target)
            .count() as u32;
        Ok(self.data_start + (nal_offset - epbs) as usize)
    }
}

/// What a wavefront decode of one slice returns, as the sequential CTB
/// loop does: the tile-scan address after the slice's last CTB, the WPP
/// context state saved last (after the second CTB of a row), and the
/// contexts after the last CTB.
pub(crate) type SliceResult = Result<(u32, Option<CabacContexts>, CabacContexts), DecodeError>;

/// How one row ended.
enum RowEnd {
    /// Decoded to the end of the row; the slice continues below.
    Complete,
    /// The slice segment ended after CTB `col` (`end_of_slice_segment_flag`).
    Ended {
        col: usize,
    },
    Failed(DecodeError),
}

struct RowOutcome {
    end: RowEnd,
    /// Columns this row started decoding (including a failed CTB), for
    /// undoing a discarded row.
    started: std::ops::Range<usize>,
    /// End of the columns decoded successfully (from `started.start`).
    decoded_end: usize,
    /// Contexts after the row's last decoded CTB.
    contexts: CabacContexts,
    /// The worker that decoded the row.
    worker: usize,
}

/// Whether a slice can be decoded with [`decode_rows`]: WPP without tiles,
/// more than one CTB row and column, and no chroma QP offset lists
/// (whose `CuQpOffsetCb` / `CuQpOffsetCr` this decoder carries across
/// rows, which a parallel decode could not reproduce).
pub(crate) fn applies(sps: &Sps, pps: &Pps, sh: &SliceHeader, substreams: usize) -> bool {
    pps.entropy_coding_sync_enabled_flag
        && !pps.tiles_enabled_flag
        && !sh.cu_chroma_qp_offset_enabled_flag
        && sps.pic_width_in_ctbs_y() >= 2
        && substreams >= 2
}

/// Decode the CTB rows of one WPP slice segment on up to `threads` threads.
///
/// `contexts` are the contexts the first CTB starts with and `seed` the
/// WPP state saved before this segment (as in the sequential loop). The
/// slice starts at raster address `start_rs` (tile scan equals raster scan
/// without tiles). The result, and every table and sample of the picture,
/// equal those of the sequential CTB loop.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_rows<P: Pixel>(
    state: &mut PictureState<P>,
    ctx: &SliceContext,
    substreams: &Substreams,
    start_rs: u32,
    contexts: CabacContexts,
    seed: Option<CabacContexts>,
    threads: usize,
) -> SliceResult {
    let (sps, sh) = (ctx.sps, ctx.sh);
    let w = sps.pic_width_in_ctbs_y() as usize;
    let h = sps.pic_height_in_ctbs_y() as usize;
    let (r0, c0) = (start_rs as usize / w, start_rs as usize % w);
    let rows = r0..(r0 + substreams.count()).min(h);
    let n_workers = threads.clamp(1, rows.len());

    // Rows above the slice are finished; the first row up to the slice start.
    let progress = Arc::new(RowProgress::new((0..h).map(|r| {
        if r < r0 {
            w
        } else if r == r0 {
            c0
        } else {
            0
        }
    })));
    // WPP state saved after the second CTB of each row (spec 9.3.2.4); the
    // first row starts with the state saved before this segment.
    let saved: Vec<Mutex<Option<CabacContexts>>> = (0..h)
        .map(|r| Mutex::new(if r == r0 { seed } else { None }))
        .collect();
    let saved_above = |r: usize| -> Option<CabacContexts> {
        if r == r0 {
            seed
        } else {
            *saved[r - 1].lock().unwrap()
        }
    };

    // SAFETY: the worker states are views of `state`'s tables. They are
    // moved into the scoped threads below and returned from them, and all
    // dropped before `state` is used again (end of this block). Every
    // access they make follows the wavefront protocol: a worker writes only
    // the CTB it is decoding and reads its own row or CTBs of rows above
    // that `progress` reports finished (it waits for column `c + 1` of the
    // row above before decoding column `c`); in test builds `AccessCheck`
    // asserts this for every access.
    // The CTB at which the sequential loop would find a complete row (the
    // segment overlaps CTBs decoded before it) and stop: the same here.
    let frozen_from = state
        .decoded_rows
        .first_frozen(start_rs as usize)
        .unwrap_or(usize::MAX);
    let workers: Vec<PictureState<P>> = (0..n_workers)
        .map(|_| unsafe { state.worker_view() })
        .collect();

    let outcomes: Vec<Mutex<Option<RowOutcome>>> = (0..h).map(|_| Mutex::new(None)).collect();

    let decode_row = |ws: &mut PictureState<P>, r: usize, worker: usize| -> RowOutcome {
        // A panic in this row (a bug) must not leave the rows below waiting
        // forever: mark the row failed while unwinding, so the scope ends
        // and re-raises the panic.
        struct FailOnPanic<'a>(&'a RowProgress, usize);
        impl Drop for FailOnPanic<'_> {
            fn drop(&mut self) {
                if std::thread::panicking() {
                    self.0.fail(self.1);
                }
            }
        }
        let _guard = FailOnPanic(&progress, r);
        #[cfg(test)]
        let check = {
            let c = Arc::new(crate::shared::AccessCheck::new(Arc::clone(&progress)));
            ws.attach_checks(&c);
            c
        };
        let first = r == r0;
        let col0 = if first { c0 } else { 0 };
        let mut outcome = RowOutcome {
            end: RowEnd::Complete,
            started: col0..col0,
            decoded_end: col0,
            contexts,
            worker,
        };
        let fail = |outcome: &mut RowOutcome, e: DecodeError| {
            outcome.end = RowEnd::Failed(e);
        };
        // Substream for this row, and its starting contexts.
        // Opened as the sequential loop does (so a bad entry point fails
        // with the same error): the first row at the slice data, later rows
        // by re-initialising at their entry point.
        let opened = substreams.start(r - r0).and_then(|o| {
            let mut c = CabacReader::new(substreams.rbsp, substreams.data_start)?;
            if !first {
                c.reinit_at(o)?;
            }
            Ok(c)
        });
        let mut cabac = match opened {
            Ok(c) => c,
            Err(e) => {
                fail(&mut outcome, e);
                return outcome;
            }
        };
        let mut ctxs = if first {
            contexts
        } else {
            // The CTB above-right of the row start, (r - 1, 1), must be
            // finished: its slice address decides the synchronisation.
            if !progress.wait(r - 1, 2) {
                outcome.end = RowEnd::Failed(DecodeError::InvalidSyntax("discarded"));
                return outcome;
            }
            let tr = (r - 1) * w + 1;
            if ws.tab_slice_addr_rs[tr] == ctx.recorded_slice_addr {
                match saved_above(r) {
                    Some(s) => s,
                    None => {
                        fail(
                            &mut outcome,
                            DecodeError::InvalidSyntax(
                                "WPP row start without a saved context state",
                            ),
                        );
                        return outcome;
                    }
                }
            } else {
                CabacContexts::init(sh.slice_qp_y, sh.slice_type, sh.cabac_init_flag)
            }
        };
        if !first {
            ws.first_qp_group = true;
        }
        for c in col0..w {
            if r > r0 && !progress.wait(r - 1, (c + 2).min(w)) {
                outcome.end = RowEnd::Failed(DecodeError::InvalidSyntax("discarded"));
                break;
            }
            if r * w + c >= frozen_from {
                fail(
                    &mut outcome,
                    DecodeError::InvalidSyntax("slice segment overlaps CTB rows already decoded"),
                );
                break;
            }
            outcome.started.end = c + 1;
            #[cfg(test)]
            check.set_current(Some((r, c)));
            let rs = (r * w + c) as u32;
            let result = decode_ctb(&mut cabac, &mut ctxs, ws, ctx, rs, rs);
            #[cfg(test)]
            check.set_current(None);
            let more = match result {
                Ok(m) => {
                    outcome.decoded_end = c + 1;
                    m
                }
                Err(e) => {
                    fail(&mut outcome, e);
                    break;
                }
            };
            if c == 1 {
                *saved[r].lock().unwrap() = Some(ctxs);
            }
            progress.publish(r, c + 1);
            if !more {
                outcome.end = RowEnd::Ended { col: c };
                break;
            }
        }
        outcome.contexts = ctxs;
        if !matches!(outcome.end, RowEnd::Complete) {
            // Rows below are never reached by a sequential decode.
            progress.fail(r);
        }
        outcome
    };

    let mut workers = std::thread::scope(|s| {
        let handles: Vec<_> = workers
            .into_iter()
            .enumerate()
            .map(|(k, mut ws)| {
                let (rows, decode_row, outcomes, progress) =
                    (rows.clone(), &decode_row, &outcomes, &progress);
                s.spawn(move || {
                    for r in rows.skip(k).step_by(n_workers) {
                        if progress.failed_above(r) {
                            break;
                        }
                        let o = decode_row(&mut ws, r, k);
                        *outcomes[r].lock().unwrap() = Some(o);
                    }
                    ws
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("wavefront worker panicked"))
            .collect::<Vec<_>>()
    });

    // The slice ends at the first row (in decoding order) that did not run
    // to its end; rows below it are discarded.
    let mut result = None;
    let mut stop = rows.end - 1;
    // CTBs decoded, per row up to the stop (a failed CTB does not count).
    let mut decoded: Vec<(usize, std::ops::Range<usize>)> = Vec::new();
    for r in rows.clone() {
        let o = outcomes[r]
            .lock()
            .unwrap()
            .take()
            .expect("every row up to the stop is decoded");
        decoded.push((r, o.started.start..o.decoded_end));
        let last_row = r + 1 == rows.end;
        let end = match o.end {
            RowEnd::Complete if !last_row => continue,
            RowEnd::Complete => {
                // The last substream ran to the end of its row without
                // ending the slice: what the sequential loop reports.
                Err(DecodeError::InvalidSyntax(if r + 1 == h {
                    "slice did not end on terminate bin"
                } else {
                    "slice missing entry_point_offset for substream"
                }))
            }
            RowEnd::Ended { col } => {
                let saved_state = (r0..=r)
                    .rev()
                    .find_map(|q| *saved[q].lock().unwrap())
                    .or(seed);
                Ok(((r * w + col + 1) as u32, saved_state, o.contexts))
            }
            RowEnd::Failed(e) => Err(e),
        };
        result = Some((end, o.worker));
        stop = r;
        break;
    }
    let (result, last_worker) = result.expect("the last row always stops");

    // The views end here, before `state` is used again; the workers keep
    // only their per-thread decoding state.
    for ws in &mut workers {
        ws.release_views();
    }
    state.take_worker_state(&workers[last_worker], &workers);
    drop(workers);
    let mut rows_completed = false;
    for (r, cols) in decoded {
        for c in cols {
            rows_completed |= state.decoded_rows.mark(r * w + c);
        }
    }
    if rows_completed && let Some(mut filter) = state.row_filter.take() {
        filter.rows_decoded(state);
        state.row_filter = Some(filter);
    }

    // Discard what the rows below the stop decoded.
    for (r, outcome) in outcomes.iter().enumerate().take(rows.end).skip(stop + 1) {
        if let Some(o) = outcome.lock().unwrap().take() {
            state.reset_ctbs(r, o.started);
        } else {
            let done = progress.done(r);
            if done > 0 {
                state.reset_ctbs(r, 0..done);
            }
        }
    }
    result
}
