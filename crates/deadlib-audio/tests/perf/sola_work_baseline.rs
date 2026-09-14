//! SOLA (Synchronized Overlap-Add) time-stretcher used to implement
//! `RateModPreservesPitch` for the music stream.
//!
//! This is a 1:1 port of `ITGMania`'s `RageSoundReader_SpeedChange` (Glenn
//! Maynard, 2006, MIT-licensed). It changes the duration of a buffered audio
//! stream without changing its pitch by finding the source position whose
//! correlation with the recent output window is maximal and linearly
//! crossfading from the previous window into the new one.
//!
//! Architecture: the decoder pushes interleaved `i16` source frames in via
//! [`SolaStretcher::push_interleaved_i16`]; the next stage pulls planar `f32`
//! stretched frames out via [`SolaStretcher::pull`]. Source frames stay buffered
//! until the SOLA algorithm has slid past them.
//!
//! Algorithm parameters match upstream:
//! * `WINDOW_SIZE_MS = 30`
//! * `tolerance = window / 4`
//! * `correlate_to_match = window / 4`
//! * `uncorrelated_to_match = tolerance + correlate_to_match = window / 2`
//! * Linear crossfade across the whole window between the previous and current
//!   correlated positions.
//! * Fractional-frame error accumulator preserves long-term ratio exactly.
//!
//! Deliberate deviations from a strict 1:1 port (all preserve or improve output
//! fidelity; none change the long-term ratio):
//! * The L1 correlation search scans the *inclusive* offset range
//!   `0..=(buffer - correlate)`. Upstream stops one short of the final valid
//!   offset (`i < iBufferDistanceToSearch`), so its best-match could never land
//!   on the last position. See [`find_closest_match`].
//! * The search compares every frame (stride 1). Upstream subsamples the
//!   per-channel planar buffer by `iStride = channel count`; at our sample rates
//!   the full-resolution search costs only a few million abs-ops/sec and gives a
//!   slightly cleaner match.
//! * The EOF flush ([`SolaStretcher::finish`]) is bounds-checked rather than
//!   relying on reads into over-allocated buffer capacity the way the C++ does.

const WINDOW_SIZE_MS: u32 = 30;
const I16_SCALE: f32 = 1.0 / 32768.0;

#[inline]
fn append_mono_i16(data: &mut Vec<f32>, interleaved: &[i16]) {
    data.extend(
        interleaved
            .iter()
            .map(|sample| f32::from(*sample) * I16_SCALE),
    );
}

#[inline]
fn append_stereo_i16(left: &mut Vec<f32>, right: &mut Vec<f32>, interleaved: &[i16]) {
    let frames = interleaved.as_chunks::<2>().0;
    left.extend(frames.iter().map(|frame| f32::from(frame[0]) * I16_SCALE));
    right.extend(frames.iter().map(|frame| f32::from(frame[1]) * I16_SCALE));
}

#[inline]
fn append_crossfade(prev: &[f32], current: &[f32], weights: &[f32], out: &mut Vec<f32>) {
    debug_assert_eq!(prev.len(), current.len());
    debug_assert_eq!(prev.len(), weights.len());
    out.reserve(prev.len());
    if std::ptr::eq(prev.as_ptr(), current.as_ptr()) {
        out.extend_from_slice(current);
        return;
    }
    append_crossfade_calculated(prev, current, weights, out);
}

#[inline]
fn append_crossfade_calculated(prev: &[f32], current: &[f32], weights: &[f32], out: &mut Vec<f32>) {
    for ((&a, &b), &t) in prev.iter().zip(current).zip(weights) {
        out.push((b - a).mul_add(t, a));
    }
}

#[inline(always)]
fn prefix_can_satisfy_reserve<T>(
    buffer: &Vec<T>,
    data_start: usize,
    live: usize,
    additional: usize,
) -> bool {
    data_start > 0
        && buffer.len().saturating_add(additional) > buffer.capacity()
        && live.saturating_add(additional) <= buffer.capacity()
}

fn compact_buffer<T: Copy>(buffer: &mut Vec<T>, data_start: usize, live: usize) {
    buffer.copy_within(data_start.., 0);
    buffer.truncate(live);
}

#[inline(always)]
fn cursor_avail_cached(
    window_frames: usize,
    pos: usize,
    data_avail_frames: usize,
    max_correlated_pos: usize,
    max_last_correlated_pos: usize,
) -> usize {
    let furthest = max_correlated_pos.max(max_last_correlated_pos);
    window_frames.saturating_sub(pos).min(
        data_avail_frames
            .saturating_sub(furthest)
            .saturating_sub(pos),
    )
}

#[inline(always)]
fn max_needed_cached(
    prospective_uncorrelated: usize,
    tolerance_frames: usize,
    window_frames: usize,
    max_correlated_pos: usize,
    pos: usize,
) -> usize {
    (prospective_uncorrelated + tolerance_frames + window_frames)
        .max(max_correlated_pos + pos + window_frames)
}

#[inline(always)]
fn earliest_cached(uncorrelated_pos: usize, min_correlated_pos: usize) -> usize {
    uncorrelated_pos.min(min_correlated_pos)
}

struct ChannelState {
    data: Vec<f32>,
    correlated_pos: usize,
    last_correlated_pos: usize,
}

pub struct SolaStretcher {
    channels: usize,
    sample_rate: u32,
    window_frames: usize,
    tolerance_frames: usize,
    /// Decoder-worker-owned crossfade table for this stretcher's lifetime.
    /// It is single-threaded, contains exactly `window_frames` entries, and is
    /// fully warmed by `new` before streaming. Indexed reads cannot miss or
    /// grow it; there is no eviction, pruning, or gameplay-thread work, and it
    /// is freed with the stretcher on the decoder worker. Per emitted sample,
    /// the worst-case table cost is one bounded load. `sola_hot_paths` records
    /// its throughput and reports the one startup allocation separately.
    fade_weights: Box<[f32]>,

    state: Vec<ChannelState>,
    /// Cached extrema for the per-channel cursors. The correlation search is
    /// the only operation that can make channel cursors diverge; maintaining
    /// the extrema there turns three decoder-hot `state` scans into scalar
    /// comparisons between searches.
    min_correlated_pos: usize,
    max_correlated_pos: usize,
    max_last_correlated_pos: usize,
    data_avail_frames: usize,
    /// Physical index in each channel's `data` Vec where logical frame 0 lives.
    /// `erase_front` advances this cursor instead of memmoving the buffer; the
    /// dead prefix is reclaimed lazily by [`compact`] once it grows past
    /// `compact_threshold_frames`. Every logical position (`*_pos`, `pos`) is
    /// relative to logical frame 0, so the physical sample is
    /// `data[data_start + logical]` and the invariant
    /// `data.len() == data_start + data_avail_frames` holds for every channel.
    data_start: usize,
    compact_threshold_frames: usize,
    uncorrelated_pos: usize,
    pos: usize,

    speed_ratio: f32,
    trailing_speed_ratio: f32,
    error_frames: f32,

    /// When set, [`step`] emits whatever source remains from the current
    /// correlated position instead of stalling for a full search window. The
    /// caller flips this on via [`finish`] once the source is exhausted so the
    /// final partial window (~`window + tolerance` frames) is not dropped.
    finishing: bool,
}

impl SolaStretcher {
    pub(super) fn new(channels: usize, sample_rate: u32) -> Self {
        debug_assert!(channels > 0);
        debug_assert!(sample_rate > 0);
        let window_frames = ((WINDOW_SIZE_MS as usize) * (sample_rate as usize) / 1000).max(64);
        let tolerance_frames = window_frames / 4;
        let fade_weights = (0..window_frames)
            .map(|frame| frame as f32 / window_frames as f32)
            .collect();
        // Reclaim the erased prefix only after it grows to several windows, so
        // the compaction memmove is amortized across many `erase_front` calls
        // instead of moving the live region every single window like `drain`.
        let compact_threshold_frames = (window_frames * 8).max(4096);
        let mut state = Vec::with_capacity(channels);
        for _ in 0..channels {
            state.push(ChannelState {
                data: Vec::new(),
                correlated_pos: 0,
                last_correlated_pos: 0,
            });
        }
        Self {
            channels,
            sample_rate,
            window_frames,
            tolerance_frames,
            fade_weights,
            state,
            min_correlated_pos: 0,
            max_correlated_pos: 0,
            max_last_correlated_pos: 0,
            data_avail_frames: 0,
            data_start: 0,
            compact_threshold_frames,
            uncorrelated_pos: 0,
            pos: 0,
            speed_ratio: 1.0,
            trailing_speed_ratio: 1.0,
            error_frames: 0.0,
            finishing: false,
        }
    }

    #[allow(dead_code)]
    pub(super) const fn channels(&self) -> usize {
        self.channels
    }

    #[allow(dead_code)]
    pub(super) const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[allow(dead_code)]
    pub(super) const fn window_frames(&self) -> usize {
        self.window_frames
    }

    #[allow(dead_code)]
    pub(super) const fn tolerance_frames(&self) -> usize {
        self.tolerance_frames
    }

    /// Current speed ratio that has been committed to the next block. SOLA
    /// reads `trailing_ratio * window` source frames per output window, so this
    /// is the ratio actually being applied to the output frames produced by the
    /// next [`pull`].
    #[allow(dead_code)]
    pub(super) const fn trailing_speed_ratio(&self) -> f32 {
        self.trailing_speed_ratio
    }

    pub(super) fn set_speed_ratio(&mut self, ratio: f32) {
        let ratio = if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            1.0
        };
        self.speed_ratio = ratio;
        // If we haven't read any data yet, put the new ratio into effect immediately.
        if self.data_avail_frames == 0 {
            self.trailing_speed_ratio = ratio;
        }
    }

    pub(super) fn reset(&mut self) {
        self.trailing_speed_ratio = self.speed_ratio;
        self.data_avail_frames = 0;
        self.data_start = 0;
        for ch in &mut self.state {
            ch.data.clear();
            ch.correlated_pos = 0;
            ch.last_correlated_pos = 0;
        }
        self.min_correlated_pos = 0;
        self.max_correlated_pos = 0;
        self.max_last_correlated_pos = 0;
        self.uncorrelated_pos = 0;
        self.pos = 0;
        self.error_frames = 0.0;
        self.finishing = false;
    }

    /// Signal that no more source will be pushed. The next [`pull`] calls drain
    /// the final partial window instead of stalling for a full search window,
    /// mirroring upstream's EOF branch in `RageSoundReader_SpeedChange::Step`.
    /// Cleared by [`reset`] before decoding another loop iteration.
    pub(super) const fn finish(&mut self) {
        self.finishing = true;
    }

    /// Total source frames currently buffered for the SOLA search window.
    #[allow(dead_code)]
    pub(super) const fn buffered_source_frames(&self) -> usize {
        self.data_avail_frames
    }

    /// Push raw interleaved i16 source frames into the search buffer.
    pub(super) fn push_interleaved_i16(&mut self, interleaved: &[i16]) {
        if interleaved.is_empty() || self.channels == 0 {
            return;
        }
        debug_assert_eq!(interleaved.len() % self.channels, 0);
        let frames = interleaved.len() / self.channels;
        self.reserve_source_frames(frames);
        match self.channels {
            1 => append_mono_i16(&mut self.state[0].data, interleaved),
            2 => {
                let (left, right) = self.state.split_at_mut(1);
                append_stereo_i16(&mut left[0].data, &mut right[0].data, interleaved);
            }
            _ => {
                for frame in interleaved.chunks_exact(self.channels) {
                    for (ch, sample) in self.state.iter_mut().zip(frame) {
                        ch.data.push(f32::from(*sample) * I16_SCALE);
                    }
                }
            }
        }
        self.data_avail_frames += frames;
    }

    /// Pull up to `max_frames` stretched output frames into `output` (planar
    /// f32 per channel). Frames are *appended* to each `output[c]` Vec.
    /// Returns the number of frames actually appended; will be less than
    /// `max_frames` if the internal buffer doesn't yet hold enough source
    /// samples for SOLA to make progress.
    pub(super) fn pull(&mut self, output: &mut [Vec<f32>], max_frames: usize) -> usize {
        debug_assert_eq!(output.len(), self.channels);
        if max_frames == 0 {
            return 0;
        }
        let mut produced = 0usize;
        let mut remaining = max_frames;
        loop {
            let cursor_avail = self.cursor_avail();
            if cursor_avail == 0 {
                if !self.step() {
                    return produced;
                }
                continue;
            }
            let n = cursor_avail.min(remaining);
            self.emit(output, n);
            produced += n;
            remaining -= n;
            if remaining == 0 {
                return produced;
            }
        }
    }

    fn cursor_avail(&self) -> usize {
        // Bound by both the current and the previous correlated position:
        // `emit` reads a window from each, and in the EOF-flush path (where no
        // fresh search runs) the previous position can sit ahead of the current
        // one. The cached maxima are updated whenever either cursor changes.
        cursor_avail_cached(
            self.window_frames,
            self.pos,
            self.data_avail_frames,
            self.max_correlated_pos,
            self.max_last_correlated_pos,
        )
    }

    fn emit(&mut self, output: &mut [Vec<f32>], frames: usize) {
        let base = self.data_start;
        let weights = &self.fade_weights[self.pos..self.pos + frames];
        for (ch, out) in self.state.iter().zip(output.iter_mut()) {
            let cur = base + ch.correlated_pos + self.pos;
            let prev = base + ch.last_correlated_pos + self.pos;
            let cur_slice = &ch.data[cur..cur + frames];
            let prev_slice = &ch.data[prev..prev + frames];
            append_crossfade(prev_slice, cur_slice, weights, out);
        }
        self.pos += frames;
    }

    /// Reclaim a dead logical prefix before growing channel storage when the
    /// retained allocation already has enough room for the live frames and the
    /// incoming packet. This keeps decoder packet-size jitter from turning
    /// erased history into avoidable reallocations between periodic compactions.
    fn reserve_source_frames(&mut self, frames: usize) {
        let reuse_prefix = self.state.iter().any(|channel| {
            prefix_can_satisfy_reserve(
                &channel.data,
                self.data_start,
                self.data_avail_frames,
                frames,
            )
        });
        if reuse_prefix {
            self.compact_storage();
        }
        for channel in &mut self.state {
            channel.data.reserve(frames);
        }
    }

    /// Returns false if the search buffer doesn't yet have enough data to
    /// compute the next window. In that case the caller must push more source
    /// before calling `pull` again — and crucially, no state is mutated, so the
    /// call is fully retryable.
    fn step(&mut self) -> bool {
        // First step after reset / EOF flush: we have data but no window yet.
        // The initial correlated_pos is 0 for every channel (matches
        // RageSoundReader_SpeedChange::Reset()), so the very first window
        // simply emits frames [0, window_frames) without crossfade (both
        // correlated and last_correlated are 0 so the LERP is a no-op).
        if self.data_avail_frames == 0 {
            return false;
        }

        // Compute the positions the advance *would* move us to, without
        // committing. The data-sufficiency check must be retryable: if there
        // isn't enough buffered source for the upcoming search we return false
        // having mutated nothing, so the decoder can push another packet and
        // call us again.
        //
        // This ordering matters. An earlier version advanced `uncorrelated_pos`
        // and `correlated_pos` *before* the data check and returned false after.
        // Under the real decoder feed (small packets, capped pull buffer) the
        // search then never had enough lookahead, so `uncorrelated_pos` ran
        // away while the read cursor walked the buffer 1:1 — collapsing the
        // stretcher into a passthrough that produced `input` frames instead of
        // `input / ratio`, i.e. no time-stretch at all.
        let advancing = self.pos > 0;
        let advance =
            (self.window_frames as f32).mul_add(self.trailing_speed_ratio, self.error_frames);
        let rounded = advance.round();
        let prospective_uncorrelated = if advancing {
            let int_advance = rounded as isize;
            if int_advance >= 0 {
                self.uncorrelated_pos.saturating_add(int_advance as usize)
            } else {
                self.uncorrelated_pos
                    .saturating_sub((-int_advance) as usize)
            }
        } else {
            self.uncorrelated_pos
        };

        // `max_needed > data_avail` is invariant under the later `erase_front`
        // (both sides shift down by the same `earliest`), so we can evaluate it
        // here against the prospective, pre-erase positions and cache the
        // boolean for reuse after the commit.
        let max_needed = max_needed_cached(
            prospective_uncorrelated,
            self.tolerance_frames,
            self.window_frames,
            self.max_correlated_pos,
            self.pos,
        );
        let insufficient = max_needed > self.data_avail_frames;
        if insufficient && !self.finishing {
            // Retryable: nothing mutated. Caller must push more source.
            return false;
        }

        // Commit the advance.
        if advancing {
            for ch in &mut self.state {
                ch.correlated_pos = ch.correlated_pos.saturating_add(self.pos);
            }
            self.min_correlated_pos = self.min_correlated_pos.saturating_add(self.pos);
            self.max_correlated_pos = self.max_correlated_pos.saturating_add(self.pos);
            self.error_frames = advance - rounded;
            self.uncorrelated_pos = prospective_uncorrelated;
            self.pos = 0;
        }

        // Commit any new speed ratio.
        self.trailing_speed_ratio = self.speed_ratio;

        // Erase data older than min(uncorrelated_pos, all correlated_pos).
        let earliest = earliest_cached(self.uncorrelated_pos, self.min_correlated_pos);
        if earliest > 0 {
            self.erase_front(earliest);
        }

        if insufficient {
            // Only reachable when `finishing`: EOF flush (mirrors
            // RageSoundReader_SpeedChange::Step's EOF path). There isn't enough
            // buffered source left for another search, so emit straight from the
            // current correlated position for as long as data lasts instead of
            // dropping the tail. No new search means `last_correlated_pos` is
            // left as-is; the `last_correlated`-aware `cursor_avail` keeps the
            // crossfade read in bounds. Returns false once nothing remains.
            self.uncorrelated_pos = self.state.first().map_or(0, |c| c.correlated_pos);
            return self.cursor_avail() > 0;
        }

        // Per-channel correlation search. Stereo is overwhelmingly the common
        // stream layout, so share its candidate and sample loops while keeping
        // independent scores and selected offsets for the two channels.
        let correlate_to_match = self.window_frames / 4;
        let uncorrelated_to_match = self.tolerance_frames + correlate_to_match;
        let base = self.data_start;
        let uncorrelated_pos = self.uncorrelated_pos;
        if let [left, right] = self.state.as_mut_slice() {
            let unc = base + uncorrelated_pos;
            let left_cor = base + left.correlated_pos;
            let right_cor = base + right.correlated_pos;
            let (left_best, right_best) = find_closest_match_stereo(
                &left.data[unc..unc + uncorrelated_to_match],
                &left.data[left_cor..left_cor + correlate_to_match],
                &right.data[unc..unc + uncorrelated_to_match],
                &right.data[right_cor..right_cor + correlate_to_match],
            );
            left.last_correlated_pos = left.correlated_pos;
            right.last_correlated_pos = right.correlated_pos;
            left.correlated_pos = left_best + uncorrelated_pos;
            right.correlated_pos = right_best + uncorrelated_pos;
            self.max_last_correlated_pos = self.max_correlated_pos;
            self.min_correlated_pos = left.correlated_pos.min(right.correlated_pos);
            self.max_correlated_pos = left.correlated_pos.max(right.correlated_pos);
            debug_assert!(left.correlated_pos + self.window_frames <= self.data_avail_frames);
            debug_assert!(right.correlated_pos + self.window_frames <= self.data_avail_frames);
            return true;
        }
        // Channels share the buffer layout so we mutate state[i] in place but
        // need to read its data slice for the closest-match computation.
        self.max_last_correlated_pos = self.max_correlated_pos;
        let mut min_correlated_pos = usize::MAX;
        let mut max_correlated_pos = 0usize;
        for ch in &mut self.state {
            let unc = base + uncorrelated_pos;
            let cor = base + ch.correlated_pos;
            let best = find_closest_match(
                &ch.data[unc..unc + uncorrelated_to_match],
                &ch.data[cor..cor + correlate_to_match],
            );
            ch.last_correlated_pos = ch.correlated_pos;
            ch.correlated_pos = best + self.uncorrelated_pos;
            min_correlated_pos = min_correlated_pos.min(ch.correlated_pos);
            max_correlated_pos = max_correlated_pos.max(ch.correlated_pos);
            debug_assert!(ch.correlated_pos + self.window_frames <= self.data_avail_frames);
        }
        self.min_correlated_pos = min_correlated_pos;
        self.max_correlated_pos = max_correlated_pos;
        true
    }

    fn erase_front(&mut self, frames: usize) {
        if frames == 0 {
            return;
        }
        debug_assert!(frames <= self.data_avail_frames);
        debug_assert!(frames <= self.uncorrelated_pos);
        for ch in &mut self.state {
            debug_assert!(frames <= ch.correlated_pos);
            ch.correlated_pos -= frames;
        }
        self.min_correlated_pos -= frames;
        self.max_correlated_pos -= frames;
        // `last_correlated_pos` is intentionally left frozen here (as upstream
        // does): the next search overwrites it before any normal emit, and the
        // EOF-flush path relies on the `data_start`-relative read still landing
        // on the same source frame. Advancing the cursor instead of draining
        // keeps every logical position valid without a memmove.
        self.data_start += frames;
        self.data_avail_frames -= frames;
        self.uncorrelated_pos -= frames;
        self.compact();
    }

    /// Reclaim the dead prefix `data[..data_start]`. `copy_within` shifts the
    /// live region down by `data_start`, which is exactly the offset every
    /// logical position is read through, so no position needs adjusting. Runs
    /// only once the prefix exceeds `compact_threshold_frames` (or the buffer
    /// has drained empty), so the memmove is amortized across many windows.
    fn compact(&mut self) {
        if self.data_start == 0 {
            return;
        }
        if self.data_start < self.compact_threshold_frames && self.data_avail_frames != 0 {
            return;
        }
        self.compact_storage();
    }

    fn compact_storage(&mut self) {
        let live = self.data_avail_frames;
        for ch in &mut self.state {
            compact_buffer(&mut ch.data, self.data_start, live);
        }
        self.data_start = 0;
    }
}

/// L1-correlation search: find the offset in `buffer` (within
/// `buffer.len() - correlate.len()` positions) whose absolute-difference sum
/// against `correlate` is smallest. Returns the offset.
///
/// The scan is inclusive of the final valid offset (`0..=distance`). Upstream's
/// C++ uses `i < iBufferDistanceToSearch`, which never considers the last
/// position; matching there is a deliberate off-by-one fix (see
/// `correlation_checks_final_valid_position` test).
fn find_closest_match(buffer: &[f32], correlate: &[f32]) -> usize {
    if buffer.len() <= correlate.len() {
        return 0;
    }
    let distance = buffer.len() - correlate.len();
    let mut best_offset = 0usize;
    // Silence produces a perfect first candidate. Peel it so the mathematical
    // lower bound exits immediately without adding a condition to every
    // ordinary, nonmatching candidate.
    let first_score = correlation_score(&buffer[..correlate.len()], correlate, f32::INFINITY);
    if first_score == 0.0 {
        return 0;
    }
    let mut best_score = if first_score.is_nan() {
        f32::INFINITY
    } else {
        first_score
    };
    for i in 1..=distance {
        let frames = &buffer[i..i + correlate.len()];
        let score = correlation_score(frames, correlate, best_score);
        if score < best_score {
            best_score = score;
            best_offset = i;
        }
    }
    best_offset
}

/// Two independent L1 searches sharing candidate traversal and sample-loop
/// bookkeeping. Scores retain the scalar operation order of
/// [`find_closest_match`], so each selected offset is bit-for-bit identical to
/// running that function separately for the two channels.
fn find_closest_match_stereo(
    left_buffer: &[f32],
    left_correlate: &[f32],
    right_buffer: &[f32],
    right_correlate: &[f32],
) -> (usize, usize) {
    debug_assert_eq!(left_buffer.len(), right_buffer.len());
    debug_assert_eq!(left_correlate.len(), right_correlate.len());
    if left_buffer.len() <= left_correlate.len() {
        return (0, 0);
    }
    let distance = left_buffer.len() - left_correlate.len();
    let (left_first, right_first) = correlation_score_stereo(
        &left_buffer[..left_correlate.len()],
        left_correlate,
        &right_buffer[..right_correlate.len()],
        right_correlate,
        f32::INFINITY,
        f32::INFINITY,
    );
    let mut left_best_offset = 0usize;
    let mut right_best_offset = 0usize;
    let mut left_best_score = if left_first.is_nan() {
        f32::INFINITY
    } else {
        left_first
    };
    let mut right_best_score = if right_first.is_nan() {
        f32::INFINITY
    } else {
        right_first
    };
    let left_exact = left_first == 0.0;
    let right_exact = right_first == 0.0;
    if left_exact && right_exact {
        return (0, 0);
    }
    for offset in 1..=distance {
        let (left_score, right_score) = correlation_score_stereo(
            &left_buffer[offset..offset + left_correlate.len()],
            left_correlate,
            &right_buffer[offset..offset + right_correlate.len()],
            right_correlate,
            if left_exact { 0.0 } else { left_best_score },
            if right_exact { 0.0 } else { right_best_score },
        );
        if !left_exact && left_score < left_best_score {
            left_best_score = left_score;
            left_best_offset = offset;
        }
        if !right_exact && right_score < right_best_score {
            right_best_score = right_score;
            right_best_offset = offset;
        }
    }
    (left_best_offset, right_best_offset)
}

#[inline(always)]
fn correlation_score_stereo(
    left_frames: &[f32],
    left_correlate: &[f32],
    right_frames: &[f32],
    right_correlate: &[f32],
    left_best: f32,
    right_best: f32,
) -> (f32, f32) {
    let mut left_score = 0.0f32;
    let mut right_score = 0.0f32;
    let mut left_active = left_best > 0.0;
    let mut right_active = right_best > 0.0;
    let mut index = 0usize;
    while index + 8 <= left_correlate.len() {
        if left_active {
            left_score += (left_frames[index] - left_correlate[index]).abs();
            left_score += (left_frames[index + 1] - left_correlate[index + 1]).abs();
            left_score += (left_frames[index + 2] - left_correlate[index + 2]).abs();
            left_score += (left_frames[index + 3] - left_correlate[index + 3]).abs();
            left_score += (left_frames[index + 4] - left_correlate[index + 4]).abs();
            left_score += (left_frames[index + 5] - left_correlate[index + 5]).abs();
            left_score += (left_frames[index + 6] - left_correlate[index + 6]).abs();
            left_score += (left_frames[index + 7] - left_correlate[index + 7]).abs();
            left_active = left_score < left_best;
        }
        if right_active {
            right_score += (right_frames[index] - right_correlate[index]).abs();
            right_score += (right_frames[index + 1] - right_correlate[index + 1]).abs();
            right_score += (right_frames[index + 2] - right_correlate[index + 2]).abs();
            right_score += (right_frames[index + 3] - right_correlate[index + 3]).abs();
            right_score += (right_frames[index + 4] - right_correlate[index + 4]).abs();
            right_score += (right_frames[index + 5] - right_correlate[index + 5]).abs();
            right_score += (right_frames[index + 6] - right_correlate[index + 6]).abs();
            right_score += (right_frames[index + 7] - right_correlate[index + 7]).abs();
            right_active = right_score < right_best;
        }
        index += 8;
        if !left_active && !right_active {
            return (left_score, right_score);
        }
    }
    while index < left_correlate.len() {
        if left_active {
            left_score += (left_frames[index] - left_correlate[index]).abs();
        }
        if right_active {
            right_score += (right_frames[index] - right_correlate[index]).abs();
        }
        index += 1;
    }
    (left_score, right_score)
}

#[inline(always)]
fn correlation_score(frames: &[f32], correlate: &[f32], best_score: f32) -> f32 {
    let mut score = 0.0f32;
    let mut j = 0usize;
    while j + 8 <= correlate.len() {
        score += (frames[j] - correlate[j]).abs();
        score += (frames[j + 1] - correlate[j + 1]).abs();
        score += (frames[j + 2] - correlate[j + 2]).abs();
        score += (frames[j + 3] - correlate[j + 3]).abs();
        score += (frames[j + 4] - correlate[j + 4]).abs();
        score += (frames[j + 5] - correlate[j + 5]).abs();
        score += (frames[j + 6] - correlate[j + 6]).abs();
        score += (frames[j + 7] - correlate[j + 7]).abs();
        j += 8;
        if score >= best_score {
            return score;
        }
    }
    while j < correlate.len() {
        score += (frames[j] - correlate[j]).abs();
        j += 1;
    }
    score
}

pub(super) fn mono(buffer: &[f32], correlate: &[f32]) -> usize {
    find_closest_match(buffer, correlate)
}
pub(super) fn stereo(lb: &[f32], lc: &[f32], rb: &[f32], rc: &[f32]) -> (usize, usize) {
    find_closest_match_stereo(lb, lc, rb, rc)
}
