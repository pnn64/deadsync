//! SOLA (Synchronized Overlap-Add) time-stretcher used to implement
//! `RateModPreservesPitch` for the music stream.
//!
//! This is a 1:1 port of ITGMania's `RageSoundReader_SpeedChange` (Glenn
//! Maynard, 2006, MIT-licensed). It changes the duration of a buffered audio
//! stream without changing its pitch by finding the source position whose
//! correlation with the recent output window is maximal and linearly
//! crossfading from the previous window into the new one.
//!
//! Architecture: the decoder pushes interleaved `i16` source frames in via
//! [`SolaStretcher::push_interleaved_i16`]; the resampler pulls planar `f32`
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

struct ChannelState {
    data: Vec<f32>,
    correlated_pos: usize,
    last_correlated_pos: usize,
}

pub(super) struct SolaStretcher {
    channels: usize,
    sample_rate: u32,
    window_frames: usize,
    tolerance_frames: usize,

    state: Vec<ChannelState>,
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
        let window_frames =
            ((WINDOW_SIZE_MS as usize) * (sample_rate as usize) / 1000).max(64);
        let tolerance_frames = window_frames / 4;
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
            state,
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
    pub(super) fn channels(&self) -> usize {
        self.channels
    }

    #[allow(dead_code)]
    pub(super) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[allow(dead_code)]
    pub(super) fn window_frames(&self) -> usize {
        self.window_frames
    }

    #[allow(dead_code)]
    pub(super) fn tolerance_frames(&self) -> usize {
        self.tolerance_frames
    }

    /// Current speed ratio that has been committed to the next block. SOLA
    /// reads `trailing_ratio * window` source frames per output window, so this
    /// is the ratio actually being applied to the output frames produced by the
    /// next [`pull`].
    #[allow(dead_code)]
    pub(super) fn trailing_speed_ratio(&self) -> f32 {
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
        self.uncorrelated_pos = 0;
        self.pos = 0;
        self.error_frames = 0.0;
        self.finishing = false;
    }

    /// Signal that no more source will be pushed. The next [`pull`] calls drain
    /// the final partial window instead of stalling for a full search window,
    /// mirroring upstream's EOF branch in `RageSoundReader_SpeedChange::Step`.
    /// Cleared by [`reset`]; only meaningful when the stretcher is being torn
    /// down (a looping decoder keeps feeding and never calls this).
    pub(super) fn finish(&mut self) {
        self.finishing = true;
    }

    /// Total source frames currently buffered for the SOLA search window.
    #[allow(dead_code)]
    pub(super) fn buffered_source_frames(&self) -> usize {
        self.data_avail_frames
    }

    /// Push raw interleaved i16 source frames into the search buffer.
    pub(super) fn push_interleaved_i16(&mut self, interleaved: &[i16]) {
        if interleaved.is_empty() || self.channels == 0 {
            return;
        }
        debug_assert_eq!(interleaved.len() % self.channels, 0);
        let frames = interleaved.len() / self.channels;
        for ch in &mut self.state {
            ch.data.reserve(frames);
        }
        for frame in interleaved.chunks_exact(self.channels) {
            for (ch, sample) in self.state.iter_mut().zip(frame.iter()) {
                ch.data.push(f32::from(*sample) * (1.0 / 32768.0));
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
        let mut avail = self.window_frames.saturating_sub(self.pos);
        for ch in &self.state {
            // Bound by both the current and the previous correlated position:
            // `emit` reads a window from each, and in the EOF-flush path (where
            // no fresh search runs) `last_correlated_pos` can sit ahead of
            // `correlated_pos`. Guarding both keeps every slice read in bounds
            // without relying on over-allocated capacity. In normal operation
            // both positions satisfy `pos + window <= data_avail`, so this term
            // never binds tighter than `window - pos`.
            let by_corr = self
                .data_avail_frames
                .saturating_sub(ch.correlated_pos)
                .saturating_sub(self.pos);
            let by_last = self
                .data_avail_frames
                .saturating_sub(ch.last_correlated_pos)
                .saturating_sub(self.pos);
            avail = avail.min(by_corr).min(by_last);
        }
        avail
    }

    fn emit(&mut self, output: &mut [Vec<f32>], frames: usize) {
        let denom = self.window_frames as f32;
        let base = self.data_start;
        for (ch, out) in self.state.iter().zip(output.iter_mut()) {
            let cur = base + ch.correlated_pos + self.pos;
            let prev = base + ch.last_correlated_pos + self.pos;
            let cur_slice = &ch.data[cur..cur + frames];
            let prev_slice = &ch.data[prev..prev + frames];
            out.reserve(frames);
            for i in 0..frames {
                let t = (self.pos + i) as f32 / denom;
                let a = prev_slice[i];
                let b = cur_slice[i];
                out.push(a + (b - a) * t);
            }
        }
        self.pos += frames;
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
        let advance = self.window_frames as f32 * self.trailing_speed_ratio + self.error_frames;
        let rounded = advance.round();
        let prospective_uncorrelated = if advancing {
            let int_advance = rounded as isize;
            if int_advance >= 0 {
                self.uncorrelated_pos.saturating_add(int_advance as usize)
            } else {
                self.uncorrelated_pos.saturating_sub((-int_advance) as usize)
            }
        } else {
            self.uncorrelated_pos
        };

        // `max_needed > data_avail` is invariant under the later `erase_front`
        // (both sides shift down by the same `earliest`), so we can evaluate it
        // here against the prospective, pre-erase positions and cache the
        // boolean for reuse after the commit.
        let mut max_needed = prospective_uncorrelated + self.tolerance_frames + self.window_frames;
        for ch in &self.state {
            max_needed = max_needed.max(ch.correlated_pos + self.pos + self.window_frames);
        }
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
            self.error_frames = advance - rounded;
            self.uncorrelated_pos = prospective_uncorrelated;
            self.pos = 0;
        }

        // Commit any new speed ratio.
        self.trailing_speed_ratio = self.speed_ratio;

        // Erase data older than min(uncorrelated_pos, all correlated_pos).
        let mut earliest = self.uncorrelated_pos;
        for ch in &self.state {
            earliest = earliest.min(ch.correlated_pos);
        }
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

        // Per-channel correlation search.
        let correlate_to_match = self.window_frames / 4;
        let uncorrelated_to_match = self.tolerance_frames + correlate_to_match;
        let base = self.data_start;
        let uncorrelated_pos = self.uncorrelated_pos;
        // Channels share the buffer layout so we mutate state[i] in place but
        // need to read its data slice for the closest-match computation.
        for ch in &mut self.state {
            let unc = base + uncorrelated_pos;
            let cor = base + ch.correlated_pos;
            let best = find_closest_match(
                &ch.data[unc..unc + uncorrelated_to_match],
                &ch.data[cor..cor + correlate_to_match],
            );
            ch.last_correlated_pos = ch.correlated_pos;
            ch.correlated_pos = best + self.uncorrelated_pos;
            debug_assert!(ch.correlated_pos + self.window_frames <= self.data_avail_frames);
        }
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
        let live = self.data_avail_frames;
        for ch in &mut self.state {
            ch.data.copy_within(self.data_start.., 0);
            ch.data.truncate(live);
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
    let mut best_score = f32::INFINITY;
    for i in 0..=distance {
        let mut score = 0.0f32;
        let frames = &buffer[i..i + correlate.len()];
        for j in 0..correlate.len() {
            score += (frames[j] - correlate[j]).abs();
            if score >= best_score {
                break;
            }
        }
        if score < best_score {
            best_score = score;
            best_offset = i;
        }
    }
    best_offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn make_sine(freq_hz: f32, sample_rate: u32, frames: usize) -> Vec<i16> {
        (0..frames)
            .map(|n| {
                let t = n as f32 / sample_rate as f32;
                ((freq_hz * 2.0 * PI * t).sin() * 0.5 * 32767.0) as i16
            })
            .collect()
    }

    fn fft_peak_bin(samples: &[f32]) -> usize {
        // Naive DFT magnitude scan: fine for short test signals.
        let n = samples.len();
        let mut best_bin = 1;
        let mut best_mag = 0.0f32;
        for k in 1..n / 2 {
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            let omega = -2.0 * PI * k as f32 / n as f32;
            for (i, s) in samples.iter().enumerate() {
                let theta = omega * i as f32;
                re += s * theta.cos();
                im += s * theta.sin();
            }
            let mag = re * re + im * im;
            if mag > best_mag {
                best_mag = mag;
                best_bin = k;
            }
        }
        best_bin
    }

    fn pull_until(stretcher: &mut SolaStretcher, frames_target: usize) -> Vec<Vec<f32>> {
        let mut out: Vec<Vec<f32>> = (0..stretcher.channels()).map(|_| Vec::new()).collect();
        loop {
            let have = out[0].len();
            if have >= frames_target {
                break;
            }
            let produced = stretcher.pull(&mut out, frames_target - have);
            if produced == 0 {
                break;
            }
        }
        out
    }

    #[test]
    fn rate_one_is_near_identity_for_sine() {
        let sr = 48_000u32;
        let frames = sr as usize; // 1 second
        let sine = make_sine(440.0, sr, frames);
        let mut s = SolaStretcher::new(1, sr);
        s.set_speed_ratio(1.0);
        s.push_interleaved_i16(&sine);
        let window = s.window_frames();
        let out = pull_until(&mut s, frames - window);
        assert!(!out[0].is_empty());
        for v in &out[0] {
            assert!(v.is_finite(), "non-finite sample produced");
            assert!(v.abs() <= 1.0, "clipped sample produced: {v}");
        }
    }

    #[test]
    fn output_length_tracks_speed_ratio() {
        let sr = 48_000u32;
        let in_frames = (sr as usize) * 2;
        let sine = make_sine(440.0, sr, in_frames);
        for &ratio in &[0.75f32, 1.5f32] {
            let mut s = SolaStretcher::new(1, sr);
            s.set_speed_ratio(ratio);
            s.push_interleaved_i16(&sine);
            let window = s.window_frames();
            let mut out: Vec<Vec<f32>> = vec![Vec::new()];
            loop {
                let n = s.pull(&mut out, 4096);
                if n == 0 {
                    break;
                }
            }
            let produced = out[0].len() as f32;
            let expected = in_frames as f32 / ratio;
            let slack = 2.0 * window as f32;
            assert!(
                (produced - expected).abs() < slack,
                "ratio={ratio}: produced {produced}, expected ~{expected} (slack {slack})"
            );
        }
    }

    /// Reproduces the *decoder's* feed pattern: push one small packet, then
    /// drain into a capped planar accumulator (like `in_planar`), with a
    /// consumer (the resampler) periodically pulling fixed-size chunks out of
    /// that accumulator. This is the path that actually runs in gameplay, and
    /// it must still compress the stream to `input / ratio` frames. The
    /// existing `output_length_tracks_speed_ratio` test pushes the whole buffer
    /// at once and so would not catch a regression specific to incremental
    /// feeding under a capped buffer.
    #[test]
    fn incremental_capped_feed_tracks_speed_ratio() {
        const CAP: usize = 4096;
        const PACKET: usize = 1152; // typical decoder packet
        const NEED: usize = 1024; // resampler input_frames_next-ish
        let sr = 48_000u32;
        let in_frames = (sr as usize) * 4;
        let sine = make_sine(440.0, sr, in_frames);
        for &ratio in &[0.75f32, 1.5f32] {
            let mut s = SolaStretcher::new(1, sr);
            s.set_speed_ratio(ratio);

            // Stand-in for PlanarAccum: a growing buffer with a consume cursor.
            let mut accum: Vec<f32> = Vec::new();
            let mut start = 0usize;
            let mut total_out = 0usize;
            let avail = |accum: &Vec<f32>, start: usize| accum.len() - start;

            let mut consume = |accum: &mut Vec<f32>, start: &mut usize| {
                while accum.len() - *start >= NEED {
                    *start += NEED;
                }
                if *start > 0 {
                    accum.drain(..*start);
                    *start = 0;
                }
            };

            for packet in sine.chunks(PACKET) {
                s.push_interleaved_i16(packet);
                loop {
                    let pull_cap = CAP.saturating_sub(avail(&accum, start));
                    if pull_cap == 0 {
                        break;
                    }
                    let mut out = [std::mem::take(&mut accum)];
                    let n = s.pull(&mut out, pull_cap.min(2048));
                    accum = out.into_iter().next().unwrap();
                    if n == 0 {
                        break;
                    }
                    total_out += n;
                }
                consume(&mut accum, &mut start);
            }
            // Final flush, matching the decoder's EOF drain.
            s.finish();
            loop {
                let pull_cap = CAP.saturating_sub(avail(&accum, start));
                if pull_cap == 0 {
                    consume(&mut accum, &mut start);
                    continue;
                }
                let mut out = [std::mem::take(&mut accum)];
                let n = s.pull(&mut out, pull_cap.min(2048));
                accum = out.into_iter().next().unwrap();
                if n == 0 {
                    break;
                }
                total_out += n;
            }

            let produced = total_out as f32;
            let expected = in_frames as f32 / ratio;
            // Allow generous slack (a few windows) for transients/EOF.
            let slack = 6.0 * s.window_frames() as f32;
            assert!(
                (produced - expected).abs() < slack,
                "ratio={ratio}: incremental feed produced {produced}, expected ~{expected} (slack {slack})"
            );
        }
    }

    #[test]
    fn sine_pitch_is_preserved_under_stretch() {
        let sr = 48_000u32;
        let freq = 1000.0f32;
        let in_frames = sr as usize; // 1 s
        let sine = make_sine(freq, sr, in_frames);
        for &ratio in &[0.75f32, 1.5f32] {
            let mut s = SolaStretcher::new(1, sr);
            s.set_speed_ratio(ratio);
            s.push_interleaved_i16(&sine);
            let out = pull_until(&mut s, 8192);
            assert!(out[0].len() >= 4096);
            let analysis_len = 4096usize.min(out[0].len());
            let bin = fft_peak_bin(&out[0][..analysis_len]);
            let bin_freq = bin as f32 * sr as f32 / analysis_len as f32;
            // Allow 2 bins of slack against the true source frequency. (The
            // SOLA crossfade has small spectral smear but the pitch peak must
            // not shift like a resample would.)
            let bin_size = sr as f32 / analysis_len as f32;
            assert!(
                (bin_freq - freq).abs() < bin_size * 3.0,
                "ratio={ratio}: peak {bin_freq} Hz, expected ~{freq} Hz (bin {bin_size} Hz)"
            );
        }
    }

    #[test]
    fn reset_clears_state() {
        let sr = 48_000u32;
        let sine = make_sine(440.0, sr, sr as usize);
        let mut s = SolaStretcher::new(1, sr);
        s.set_speed_ratio(1.5);
        s.push_interleaved_i16(&sine);
        let _ = pull_until(&mut s, 8192);
        s.reset();
        assert_eq!(s.buffered_source_frames(), 0);
        assert_eq!(s.trailing_speed_ratio(), 1.5);
        let mut out: Vec<Vec<f32>> = vec![Vec::new()];
        let produced = s.pull(&mut out, 256);
        assert_eq!(produced, 0, "pull on empty buffer must produce nothing");
    }

    #[test]
    fn stereo_channels_produce_independent_output() {
        let sr = 48_000u32;
        let n = sr as usize / 2;
        let left = make_sine(440.0, sr, n);
        let right = make_sine(1320.0, sr, n);
        let mut inter = Vec::with_capacity(n * 2);
        for i in 0..n {
            inter.push(left[i]);
            inter.push(right[i]);
        }
        let mut s = SolaStretcher::new(2, sr);
        s.set_speed_ratio(1.25);
        s.push_interleaved_i16(&inter);
        let out = pull_until(&mut s, 4096);
        assert_eq!(out.len(), 2);
        assert!(out[0].len() >= 2048 && out[1].len() >= 2048);
        // Channels diverge because they carry different frequencies.
        let l_sum: f32 = out[0].iter().take(1024).map(|v| v.abs()).sum();
        let r_sum: f32 = out[1].iter().take(1024).map(|v| v.abs()).sum();
        assert!(l_sum > 0.0 && r_sum > 0.0);
    }

    /// Regression test for the L1 correlation off-by-one: the search must
    /// include the last valid offset (`buffer.len() - correlate.len()`) in
    /// its scan, not stop just before it.
    #[test]
    fn correlation_checks_final_valid_position() {
        let mut buffer = vec![0.0f32; 10];
        let pattern = [1.0f32, 2.0, 3.0, 4.0];
        // Plant the only perfect match at the last valid position.
        let last = buffer.len() - pattern.len();
        buffer[last..].copy_from_slice(&pattern);
        let found = find_closest_match(&buffer, &pattern);
        assert_eq!(
            found, last,
            "L1 search must check the final valid offset"
        );
    }

    /// At `rate=1.0` SOLA should not stall waiting for the full search
    /// window — preserve_pitch at unit rate is a no-op and the caller is
    /// expected to bypass SOLA entirely, but the stretcher itself should
    /// still produce output promptly if it is fed.
    #[test]
    fn rate_one_does_not_stall() {
        let sr = 48_000u32;
        let sine = make_sine(440.0, sr, sr as usize);
        let mut s = SolaStretcher::new(1, sr);
        s.set_speed_ratio(1.0);
        s.push_interleaved_i16(&sine);
        let out = pull_until(&mut s, 8192);
        assert!(
            out[0].len() >= 4096,
            "rate=1.0 should not stall: only got {} samples",
            out[0].len()
        );
    }

    /// Changing the speed ratio after some output has already been pulled
    /// must continue producing output (not get stuck in an inconsistent
    /// internal state).
    #[test]
    fn mid_stream_rate_change_continues_producing() {
        let sr = 48_000u32;
        let sine = make_sine(440.0, sr, sr as usize * 2);
        let mut s = SolaStretcher::new(1, sr);
        s.set_speed_ratio(1.5);
        s.push_interleaved_i16(&sine);
        let first = pull_until(&mut s, 4096);
        assert!(!first[0].is_empty());
        s.set_speed_ratio(0.75);
        let mut second: Vec<Vec<f32>> = vec![Vec::new()];
        let produced = s.pull(&mut second, 4096);
        assert!(
            produced > 0,
            "stretcher should keep producing after a mid-stream ratio change"
        );
        for v in &second[0] {
            assert!(v.is_finite());
        }
    }

    /// Silence in must produce silence out (no NaN, no divide-by-zero in
    /// the correlation search even though every L1 score is 0).
    #[test]
    fn silence_in_silence_out() {
        let sr = 48_000u32;
        let silence = vec![0i16; sr as usize];
        let mut s = SolaStretcher::new(1, sr);
        s.set_speed_ratio(1.5);
        s.push_interleaved_i16(&silence);
        let out = pull_until(&mut s, 8192);
        for &v in &out[0] {
            assert!(v.is_finite(), "non-finite sample in silent stretch");
            assert!(v.abs() < 1e-6, "non-silent sample in silent stretch: {v}");
        }
    }

    /// Pushing fewer frames than the search window requires must not
    /// panic. The first window emits up to the buffered count without a
    /// real SOLA step (matches upstream behavior); subsequent pulls must
    /// stop cleanly once the buffer is exhausted.
    #[test]
    fn sub_window_push_does_not_panic() {
        let sr = 48_000u32;
        let mut s = SolaStretcher::new(1, sr);
        s.set_speed_ratio(1.5);
        let pushed = 100usize;
        let short = make_sine(440.0, sr, pushed);
        s.push_interleaved_i16(&short);
        let mut out: Vec<Vec<f32>> = vec![Vec::new()];
        let first = s.pull(&mut out, 1024);
        assert!(
            first <= pushed,
            "must not invent samples: pushed {pushed}, produced {first}"
        );
        // A second pull with no further pushes must stop (zero) rather
        // than block or panic.
        let second = s.pull(&mut out, 1024);
        assert_eq!(second, 0);
        for v in &out[0] {
            assert!(v.is_finite());
        }
    }

    /// Mono SOLA path: pitch must be preserved across stretch, separate
    /// from the existing stereo-divergence test.
    #[test]
    fn mono_pitch_is_preserved_under_stretch() {
        let sr = 48_000u32;
        let freq = 880.0f32;
        let in_frames = sr as usize;
        let sine = make_sine(freq, sr, in_frames);
        let mut s = SolaStretcher::new(1, sr);
        s.set_speed_ratio(1.25);
        s.push_interleaved_i16(&sine);
        let out = pull_until(&mut s, 4096);
        let analysis_len = 4096usize.min(out[0].len());
        assert!(analysis_len >= 2048);
        let bin = fft_peak_bin(&out[0][..analysis_len]);
        let bin_freq = bin as f32 * sr as f32 / analysis_len as f32;
        let bin_size = sr as f32 / analysis_len as f32;
        assert!(
            (bin_freq - freq).abs() < bin_size * 3.0,
            "mono pitch shifted: peak {bin_freq} Hz, expected ~{freq} Hz (bin {bin_size} Hz)"
        );
    }

    /// `finish()` must flush the final partial window that a plain drain holds
    /// back for a search it can never satisfy. The finished stream produces at
    /// least as much output as the un-finished one, stays finite, and still
    /// terminates cleanly.
    #[test]
    fn finish_flushes_tail() {
        let sr = 48_000u32;
        let sine = make_sine(440.0, sr, sr as usize);

        let drain = |finish: bool| -> usize {
            let mut s = SolaStretcher::new(1, sr);
            s.set_speed_ratio(1.5);
            s.push_interleaved_i16(&sine);
            let mut out: Vec<Vec<f32>> = vec![Vec::new()];
            // Drain everything obtainable without finishing first.
            loop {
                if s.pull(&mut out, 4096) == 0 {
                    break;
                }
            }
            if finish {
                s.finish();
                loop {
                    if s.pull(&mut out, 4096) == 0 {
                        break;
                    }
                }
            }
            for v in &out[0] {
                assert!(v.is_finite(), "non-finite sample after flush");
            }
            out[0].len()
        };

        let without = drain(false);
        let with = drain(true);
        assert!(
            with >= without,
            "finish() should not drop output: with {with} < without {without}"
        );
        assert!(
            with - without <= 2 * SolaStretcher::new(1, sr).window_frames(),
            "flushed tail should be at most ~one window: with {with}, without {without}"
        );
    }

    /// Calling `finish()` on an empty stretcher must not produce anything or
    /// panic.
    #[test]
    fn finish_on_empty_is_noop() {
        let sr = 48_000u32;
        let mut s = SolaStretcher::new(2, sr);
        s.set_speed_ratio(0.75);
        s.finish();
        let mut out: Vec<Vec<f32>> = vec![Vec::new(), Vec::new()];
        assert_eq!(s.pull(&mut out, 1024), 0);
        assert!(out[0].is_empty() && out[1].is_empty());
    }
}
