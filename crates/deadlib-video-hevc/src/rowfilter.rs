//! Loop filters row by row while a picture decodes (frame threading).
//!
//! With frame threading a picture is referenced while it is still being
//! decoded, so its rows must become final (deblocked and SAO-filtered) as
//! decoding goes, not only at the end. [`RowFilter`] runs both filters one
//! CTB row at a time as rows finish decoding, and publishes the rows that
//! are final in the picture's progress:
//!
//! - Deblocking CTB row `r` (luma rows `[y0, y1)`) waits until row `r + 1`
//!   is decoded: intra prediction of row `r + 1` reads row `r`'s bottom
//!   samples, and the horizontal edge at the top of row `r` changes the
//!   bottom of row `r - 1`, both before loop filtering. Then its vertical
//!   edges, then its horizontal edges `y0 <= y < y1`.
//!   A horizontal edge at row `y` reads rows `y - 4 ..= y + 3` and writes
//!   `y - 3 ..= y + 2` (chroma `yc - 2 ..= yc + 1`), so it sees exactly what
//!   the whole-picture order (all vertical edges, then all horizontal ones)
//!   would show it.
//! - SAO of row `r` reads deblocked samples one row beyond it, which are
//!   final once row `r + 1` is deblocked, so it runs then (or at the end,
//!   for the last row). The SAO input is a snapshot of the deblocked rows,
//!   copied just before SAO changes them.
//! - After SAO of row `r`, rows `< y1` never change again: the next
//!   deblocking step writes rows from `y1(r + 1) - 3` on. They are
//!   published as final.
//!
//! Which CTB rows are decoded is tracked by [`DecodedRows`], in every mode:
//! a slice segment that would write into a CTB row already complete (only a
//! corrupt stream does that) is rejected, with any thread count. Here the
//! row may already be published, and other threads may be reading it.

use std::sync::Arc;

use crate::cu_tree::PictureState;
use crate::dpb::DecodedPicture;
use crate::pixel::Pixel;
use crate::pps::Pps;
use crate::shared::SharedTable;
use crate::sps::Sps;

/// The row-by-row loop filter of one picture.
pub(crate) struct RowFilter<P: Pixel> {
    picture: Arc<DecodedPicture>,
    sps: Arc<Sps>,
    pps: Arc<Pps>,
    ctb: usize,
    h_ctbs: usize,
    /// CTB rows deblocked, and through SAO (final). How many are decoded is
    /// `PictureState::decoded_rows`.
    deblocked_rows: usize,
    sao_rows: usize,
    sao: bool,
    /// Deblocked samples for SAO (whole planes, filled row by row).
    snapshot: [SharedTable<P>; 3],
}

impl<P: Pixel> RowFilter<P> {
    /// A row filter for the picture `state` decodes into (`picture` is its
    /// DPB entry, whose progress it publishes). Takes the SAO snapshot
    /// buffers of `state` (returned by `finish`).
    pub(crate) fn new(
        state: &mut PictureState<P>,
        picture: Arc<DecodedPicture>,
        sps: Arc<Sps>,
        pps: Arc<Pps>,
    ) -> Self {
        let ctb = 1usize << state.log2_ctb_size;
        let h_ctbs = (state.height as usize).div_ceil(ctb);
        let sao = sps.sample_adaptive_offset_enabled_flag;
        let lens = [
            state.y_plane.len(),
            state.u_plane.len(),
            state.v_plane.len(),
        ];
        let mut snapshot = std::mem::take(&mut state.sao_snapshot);
        if sao {
            for (s, &n) in snapshot.iter_mut().zip(&lens) {
                s.resize(n, P::zero());
            }
        }
        Self {
            picture,
            sps,
            pps,
            ctb,
            h_ctbs,
            deblocked_rows: 0,
            sao_rows: 0,
            sao,
            snapshot: snapshot.map(SharedTable::from),
        }
    }

    /// More CTB rows of the picture are decoded: filter what can be
    /// filtered and publish progress.
    pub(crate) fn rows_decoded(&mut self, state: &mut PictureState<P>) {
        self.advance(state, false);
    }

    /// Filter what can be filtered (everything when `finishing`: the CTBs
    /// no slice decoded have been filled by then) and publish progress.
    fn advance(&mut self, state: &mut PictureState<P>, finishing: bool) {
        let decoded_rows = state.decoded_rows.rows();
        // Row r is deblocked once row r + 1 is decoded (see the module
        // documentation), all of them when finishing.
        let target = if finishing {
            self.h_ctbs
        } else {
            decoded_rows.saturating_sub(1)
        };
        while self.deblocked_rows < target {
            self.deblock_row(state, self.deblocked_rows);
            self.deblocked_rows += 1;
        }
        while self.sao_rows < self.deblocked_rows
            && (self.sao_rows + 1 < self.deblocked_rows || self.deblocked_rows == self.h_ctbs)
        {
            self.sao_row(state, self.sao_rows);
            self.sao_rows += 1;
        }
        let final_rows = (self.sao_rows * self.ctb).min(state.height as usize);
        self.picture.progress.publish(decoded_rows, final_rows);
    }

    /// Finish the picture's filtering (after `fill_undecoded_ctbs`) and
    /// give `state` its SAO snapshot buffers back.
    pub(crate) fn finish(mut self, state: &mut PictureState<P>) {
        self.advance(state, true);
        state.sao_snapshot = std::mem::take(&mut self.snapshot).map(SharedTable::into_vec);
    }

    /// Luma rows `[y0, y1)` of CTB row `r`.
    fn rows(&self, state: &PictureState<P>, r: usize) -> (usize, usize) {
        let y0 = r * self.ctb;
        (y0, ((r + 1) * self.ctb).min(state.height as usize))
    }

    /// Cut each plane at `[start, end)` (in its own rows; `None`: to the
    /// end of the plane) and run `f` on the three slices.
    fn with_bands(
        state: &mut PictureState<P>,
        rows: [(usize, Option<usize>); 3],
        f: impl FnOnce(&PictureState<P>, [&mut [P]; 3]),
    ) {
        let strides = [state.y_stride, state.uv_stride, state.uv_stride];
        let mut planes = [
            std::mem::take(&mut state.y_plane),
            std::mem::take(&mut state.u_plane),
            std::mem::take(&mut state.v_plane),
        ];
        {
            let mut cut = planes.iter_mut().enumerate().map(|(p, plane)| {
                let len = plane.len();
                let (start, end) = rows[p];
                let a = (start * strides[p]).min(len);
                let b = end.map_or(len, |e| (e * strides[p]).clamp(a, len));
                plane.split_mut(&[a, b]).pop().expect("one slice")
            });
            let (y, u, v) = (
                cut.next().unwrap(),
                cut.next().unwrap(),
                cut.next().unwrap(),
            );
            f(state, [y, u, v]);
        }
        let [y, u, v] = planes;
        state.y_plane = y;
        state.u_plane = u;
        state.v_plane = v;
    }

    fn deblock_row(&self, state: &mut PictureState<P>, r: usize) {
        let (y0, y1) = self.rows(state, r);
        let last = r + 1 == self.h_ctbs;
        let sh = state.chroma_shift_h as usize;
        let (sps, pps) = (&*self.sps, &*self.pps);
        let end = |luma_row: usize, margin: usize| (!last).then(|| luma_row - margin);
        let end_c = |luma_row: usize, margin: usize| (!last).then(|| (luma_row >> sh) - margin);
        // Vertical edges: the row's own samples.
        Self::with_bands(
            state,
            [
                (y0, end(y1, 0)),
                (y0 >> sh, end_c(y1, 0)),
                (y0 >> sh, end_c(y1, 0)),
            ],
            |st, [y, u, v]| {
                crate::deblock::luma_vertical_edges(st, sps, y, y0, y0..y1);
                crate::deblock::chroma_edges::<P, true>(st, sps, pps, [u, v], y0 >> sh, y0..y1);
            },
        );
        // Horizontal edges y0 <= y < y1: rows [y0 - 4, y1 - 4) (see the
        // module documentation).
        let (hy, hc) = if r == 0 {
            (0, 0)
        } else {
            (y0 - 4, (y0 >> sh) - 4)
        };
        Self::with_bands(
            state,
            [(hy, end(y1, 4)), (hc, end_c(y1, 4)), (hc, end_c(y1, 4))],
            |st, [y, u, v]| {
                crate::deblock::luma_horizontal_edges(st, sps, y, hy, y0..y1);
                crate::deblock::chroma_edges::<P, false>(st, sps, pps, [u, v], hc, y0..y1);
            },
        );
    }

    fn sao_row(&mut self, state: &mut PictureState<P>, r: usize) {
        if !self.sao {
            return;
        }
        let (y0, y1) = self.rows(state, r);
        let last = r + 1 == self.h_ctbs;
        let sh = state.chroma_shift_h as usize;
        let strides = [state.y_stride, state.uv_stride, state.uv_stride];
        // Snapshot the deblocked rows SAO of this row reads, including the
        // first row below (the row above was copied with the previous row,
        // before its SAO).
        for (p, snap) in self.snapshot.iter_mut().enumerate() {
            let (r0, r1) = if p == 0 {
                (y0, y1 + 1)
            } else {
                (y0 >> sh, (y1 >> sh) + 1)
            };
            let len = snap.len();
            let (a, b) = ((r0 * strides[p]).min(len), (r1 * strides[p]).min(len));
            let plane = match p {
                0 => &state.y_plane,
                1 => &state.u_plane,
                _ => &state.v_plane,
            };
            snap[a..b].copy_from_slice(&plane[a..b]);
        }
        let src = [
            &self.snapshot[0][..],
            &self.snapshot[1][..],
            &self.snapshot[2][..],
        ];
        let sps = &*self.sps;
        let end = (!last).then_some(y1);
        let end_c = (!last).then_some(y1 >> sh);
        Self::with_bands(
            state,
            [(y0, end), (y0 >> sh, end_c), (y0 >> sh, end_c)],
            |st, [y, u, v]| {
                crate::sao::sao_ctb_rows(
                    st,
                    sps,
                    src,
                    [(y, y0), (u, y0 >> sh), (v, y0 >> sh)],
                    r..r + 1,
                );
            },
        );
        let block_rows = state.tab_no_filter.len() / state.min_pu_width.max(1);
        let b1 = if last { block_rows } else { y1 >> 2 };
        crate::sao::restore_no_filter(state, src, (y0 >> 2)..b1.min(block_rows));
    }
}

/// Which CTBs of a picture are decoded, and how many CTB rows from the top
/// are complete. Kept in every mode (`PictureState::decoded_rows`), so the
/// rule that a slice segment may not rewrite a complete CTB row applies
/// with any thread count. A CTB that failed does not count.
#[derive(Debug, Default)]
pub(crate) struct DecodedRows {
    w_ctbs: usize,
    decoded: Vec<bool>,
    row_done: Vec<usize>,
    rows: usize,
}

impl DecodedRows {
    pub(crate) fn new(w_ctbs: usize, h_ctbs: usize) -> Self {
        Self {
            w_ctbs,
            decoded: vec![false; w_ctbs * h_ctbs],
            row_done: vec![0; h_ctbs],
            rows: 0,
        }
    }

    /// CTB rows complete from the top of the picture.
    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    /// Whether CTB `rs` (raster address) lies in a complete CTB row.
    pub(crate) fn frozen(&self, rs: usize) -> bool {
        self.w_ctbs > 0 && rs / self.w_ctbs < self.rows
    }

    /// The first CTB at or after `start` that a slice segment starting
    /// there would find frozen (in a complete row) when it reaches it,
    /// decoding CTBs in raster order: the CTB where the sequential loop
    /// stops with an overlap error. Depends only on the CTBs decoded
    /// before the segment, so the wavefront path, which decodes the rows
    /// concurrently, stops at the same CTB.
    ///
    /// Row `r` is complete when CTB `rs` of it is reached only if every
    /// CTB above and before `rs` is decoded (by earlier segments, or by
    /// this one, which covers `[start, rs)`) and so is the rest of row `r`
    /// from `rs` on (earlier segments alone).
    pub(crate) fn first_frozen(&self, start: usize) -> Option<usize> {
        let len = self.decoded.len();
        if self.w_ctbs == 0 || self.decoded[..start.min(len)].iter().any(|&d| !d) {
            // A CTB before the segment is missing: no row from there on
            // completes before the segment ends.
            return None;
        }
        let mut row = start / self.w_ctbs;
        while row * self.w_ctbs < len {
            let (row_start, row_end) = (row * self.w_ctbs, (row + 1) * self.w_ctbs);
            // The row's decoded tail: CTBs from `tail` to the row's end.
            let tail = (row_start..row_end)
                .rev()
                .find(|&rs| !self.decoded[rs])
                .map_or(row_start, |rs| rs + 1);
            let first = tail.max(start);
            if first < row_end {
                return Some(first);
            }
            row += 1;
        }
        None
    }

    /// CTB `rs` is decoded. Returns whether that completed CTB rows.
    pub(crate) fn mark(&mut self, rs: usize) -> bool {
        if rs >= self.decoded.len() || self.decoded[rs] {
            return false;
        }
        self.decoded[rs] = true;
        self.row_done[rs / self.w_ctbs] += 1;
        let before = self.rows;
        while self.rows < self.row_done.len() && self.row_done[self.rows] == self.w_ctbs {
            self.rows += 1;
        }
        self.rows > before
    }
}

#[cfg(test)]
mod tests {
    use super::DecodedRows;

    /// `first_frozen` matches the sequential loop: decode CTBs one by one
    /// from `start`, stopping at the first one in a complete row. Random
    /// maps of CTBs decoded by earlier segments, 4×3 CTBs.
    #[test]
    fn first_frozen_matches_sequential_decoding() {
        let (w, h) = (4, 3);
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..20000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let start = (seed >> 40) as usize % (w * h);
            let mut rows = DecodedRows::new(w, h);
            for rs in 0..w * h {
                if seed >> rs & 1 == 1 {
                    rows.mark(rs);
                }
            }
            let expected = (start..w * h).find(|&rs| {
                let frozen = rows.frozen(rs);
                rows.mark(rs);
                frozen
            });
            let mut fresh = DecodedRows::new(w, h);
            for rs in 0..w * h {
                if seed >> rs & 1 == 1 {
                    fresh.mark(rs);
                }
            }
            assert_eq!(
                fresh.first_frozen(start),
                expected,
                "map {seed:x} start {start}"
            );
        }
    }
}
