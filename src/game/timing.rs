use crate::game::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use crate::game::note::{Note, NoteType};
use log::info;
use rssp::timing as rssp_timing;
use std::cmp::Ordering;
use std::sync::Arc;

// --- ITGMania Parity Constants and Helpers ---
pub const ROWS_PER_BEAT: i32 = 48;

// ------------------ Unified Timing Windows (Gameplay + Visuals) ------------------
// All base windows are in seconds.
pub const TIMING_WINDOW_ADD_S: f32 = 0.0015; // +1.5ms padding applied by ITG/SM

// ITG tap windows (seconds, exclusive of TIMING_WINDOW_ADD_S).
pub const BASE_W1_S: f32 = 0.0215;
pub const BASE_W2_S: f32 = 0.0430;
pub const BASE_W3_S: f32 = 0.1020;
pub const BASE_W4_S: f32 = 0.1350;
pub const BASE_W5_S: f32 = 0.1800;
pub const BASE_MINE_S: f32 = 0.0700;

// FA+ inner Fantastic window (W0) is defined using Simply Love's FA+ W1 timing.
// See SL.Preferences["FA+"].TimingWindowSecondsW1 in SL_Init.lua.
pub const BASE_FA_PLUS_W0_S: f32 = 0.0135;

#[derive(Copy, Clone, Debug)]
pub struct TimingProfile {
    // Unified ITG tap windows (seconds, already including TIMING_WINDOW_ADD_S), W1..W5.
    pub windows_s: [f32; 5],
    // Optional FA+ inner Fantastic window (seconds, already including TIMING_WINDOW_ADD_S).
    pub fa_plus_window_s: Option<f32>,
    // Mine window (seconds, already including TIMING_WINDOW_ADD_S).
    pub mine_window_s: f32,
}

impl TimingProfile {
    #[inline(always)]
    pub fn default_itg_with_fa_plus() -> Self {
        let windows_s = [
            BASE_W1_S + TIMING_WINDOW_ADD_S,
            BASE_W2_S + TIMING_WINDOW_ADD_S,
            BASE_W3_S + TIMING_WINDOW_ADD_S,
            BASE_W4_S + TIMING_WINDOW_ADD_S,
            BASE_W5_S + TIMING_WINDOW_ADD_S,
        ];
        let fa_plus_window_s = Some(BASE_FA_PLUS_W0_S + TIMING_WINDOW_ADD_S);
        let mine_window_s = mine_window_s();
        TimingProfile {
            windows_s,
            fa_plus_window_s,
            mine_window_s,
        }
    }

    #[inline(always)]
    pub fn windows_ms(&self) -> [f32; 5] {
        let s = self.windows_s;
        [
            s[0] * 1000.0,
            s[1] * 1000.0,
            s[2] * 1000.0,
            s[3] * 1000.0,
            s[4] * 1000.0,
        ]
    }
}

#[inline(always)]
pub fn effective_windows_ms() -> [f32; 5] {
    TimingProfile::default_itg_with_fa_plus().windows_ms()
}

#[inline(always)]
pub fn mine_window_s() -> f32 {
    BASE_MINE_S + TIMING_WINDOW_ADD_S
}

/// Classify a signed tap offset (seconds) into an ITG-style JudgeGrade and
/// detailed TimingWindow (including FA+ W0 when enabled in the profile).
///
/// Callers should ensure |offset_s| is within the outer WayOff window; if it is
/// not, the returned JudgeGrade will still be WayOff.
#[inline(always)]
pub fn classify_offset_s(offset_s: f32, profile: &TimingProfile) -> (JudgeGrade, TimingWindow) {
    let abs = offset_s.abs();
    if let Some(w0) = profile.fa_plus_window_s
        && abs <= w0
    {
        return (JudgeGrade::Fantastic, TimingWindow::W0);
    }
    let w = profile.windows_s;
    if abs <= w[0] {
        (JudgeGrade::Fantastic, TimingWindow::W1)
    } else if abs <= w[1] {
        (JudgeGrade::Excellent, TimingWindow::W2)
    } else if abs <= w[2] {
        (JudgeGrade::Great, TimingWindow::W3)
    } else if abs <= w[3] {
        (JudgeGrade::Decent, TimingWindow::W4)
    } else {
        (JudgeGrade::WayOff, TimingWindow::W5)
    }
}

#[inline(always)]
pub fn note_row_to_beat(row: i32) -> f32 {
    row as f32 / ROWS_PER_BEAT as f32
}

#[inline(always)]
pub fn beat_to_note_row(beat: f32) -> i32 {
    (beat * ROWS_PER_BEAT as f32).round() as i32
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedUnit {
    Beats,
    Seconds,
}

#[derive(Debug, Clone, Copy)]
pub struct StopSegment {
    pub beat: f32,
    pub duration: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DelaySegment {
    pub beat: f32,
    pub duration: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct WarpSegment {
    pub beat: f32,
    pub length: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SpeedSegment {
    pub beat: f32,
    pub ratio: f32,
    pub delay: f32,
    pub unit: SpeedUnit,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollSegment {
    pub beat: f32,
    pub ratio: f32,
}

#[derive(Debug, Clone, Default)]
pub struct TimingSegments {
    pub beat0_offset_adjust: f32,
    pub bpms: Vec<(f32, f32)>,
    pub stops: Vec<StopSegment>,
    pub delays: Vec<DelaySegment>,
    pub warps: Vec<WarpSegment>,
    pub speeds: Vec<SpeedSegment>,
    pub scrolls: Vec<ScrollSegment>,
    pub fakes: Vec<FakeSegment>,
}

impl From<&rssp_timing::TimingSegments> for TimingSegments {
    fn from(segments: &rssp_timing::TimingSegments) -> Self {
        let speeds = segments
            .speeds
            .iter()
            .map(|(beat, ratio, delay, unit)| SpeedSegment {
                beat: *beat,
                ratio: *ratio,
                delay: *delay,
                unit: match unit {
                    rssp_timing::SpeedUnit::Beats => SpeedUnit::Beats,
                    rssp_timing::SpeedUnit::Seconds => SpeedUnit::Seconds,
                },
            })
            .collect();

        Self {
            beat0_offset_adjust: segments.beat0_offset_adjust,
            bpms: segments.bpms.clone(),
            stops: segments
                .stops
                .iter()
                .map(|(beat, duration)| StopSegment {
                    beat: *beat,
                    duration: *duration,
                })
                .collect(),
            delays: segments
                .delays
                .iter()
                .map(|(beat, duration)| DelaySegment {
                    beat: *beat,
                    duration: *duration,
                })
                .collect(),
            warps: segments
                .warps
                .iter()
                .map(|(beat, length)| WarpSegment {
                    beat: *beat,
                    length: *length,
                })
                .collect(),
            speeds,
            scrolls: segments
                .scrolls
                .iter()
                .map(|(beat, ratio)| ScrollSegment {
                    beat: *beat,
                    ratio: *ratio,
                })
                .collect(),
            fakes: segments
                .fakes
                .iter()
                .map(|(beat, length)| FakeSegment {
                    beat: *beat,
                    length: *length,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SpeedRuntime {
    start_time: f32,
    end_time: f32,
    prev_ratio: f32,
}

#[derive(Debug, Clone, Copy)]
struct ScrollPrefix {
    beat: f32,
    cum_displayed: f32,
    ratio: f32,
}

#[derive(Debug, Clone, Default)]
pub struct TimingData {
    /// A pre-calculated mapping from a note row index to its precise beat.
    row_to_beat: Arc<Vec<f32>>,
    /// A pre-calculated mapping from a beat to its precise time in seconds.
    beat_to_time: Arc<Vec<BeatTimePoint>>,
    stops: Vec<StopSegment>,
    delays: Vec<DelaySegment>,
    warps: Vec<WarpSegment>,
    speeds: Vec<SpeedSegment>,
    scrolls: Vec<ScrollSegment>,
    fakes: Vec<FakeSegment>,
    speed_runtime: Vec<SpeedRuntime>,
    scroll_prefix: Vec<ScrollPrefix>,
    global_offset_sec: f32,
    max_bpm: f32,
}

#[derive(Debug, Clone, Default, Copy)]
struct BeatTimePoint {
    beat: f32,
    time_sec: f32,
    bpm: f32,
}

#[derive(Debug, Clone, Copy)]
struct GetBeatStarts {
    bpm_idx: usize,
    stop_idx: usize,
    delay_idx: usize,
    warp_idx: usize,
    last_row: i32,
    last_time: f32,
    warp_destination: f32,
    is_warping: bool,
}

impl Default for GetBeatStarts {
    fn default() -> Self {
        Self {
            bpm_idx: 0,
            stop_idx: 0,
            delay_idx: 0,
            warp_idx: 0,
            last_row: 0,
            last_time: 0.0,
            warp_destination: 0.0,
            is_warping: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BeatInfoCache {
    start: GetBeatStarts,
    last_elapsed_time: f32,
    global_offset_sec: f32,
}

impl BeatInfoCache {
    pub fn new(timing: &TimingData) -> Self {
        let mut cache = Self {
            start: GetBeatStarts::default(),
            last_elapsed_time: f32::NEG_INFINITY,
            global_offset_sec: timing.global_offset_sec,
        };
        cache.start.last_time =
            -timing.beat0_offset_seconds() - timing.beat0_group_offset_seconds();
        cache
    }

    pub fn reset(&mut self, timing: &TimingData) {
        self.start = GetBeatStarts::default();
        self.start.last_time = -timing.beat0_offset_seconds() - timing.beat0_group_offset_seconds();
        self.last_elapsed_time = f32::NEG_INFINITY;
        self.global_offset_sec = timing.global_offset_sec;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GetBeatArgs {
    pub elapsed_time: f32,
    pub beat: f32,
    pub bps_out: f32,
    pub warp_dest_out: f32,
    pub warp_begin_out: i32,
    pub freeze_out: bool,
    pub delay_out: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BeatInfo {
    pub beat: f32,
    pub is_in_freeze: bool,
    pub is_in_delay: bool,
}

#[derive(PartialEq, Eq)]
enum TimingEvent {
    Bpm,
    Stop,
    Delay,
    StopDelay,
    Warp,
    WarpDest,
    Marker,
    NotFound,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FakeSegment {
    pub beat: f32,
    pub length: f32,
}

impl TimingData {
    pub fn from_segments(
        song_offset_sec: f32,
        global_offset_sec: f32,
        segments: &TimingSegments,
        row_to_beat: &[f32],
    ) -> Self {
        let mut parsed_bpms = segments.bpms.clone();
        if parsed_bpms.is_empty() {
            parsed_bpms.push((0.0, 60.0));
        }
        parsed_bpms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Less));

        let mut stops = segments.stops.clone();
        let mut delays = segments.delays.clone();
        let mut warps = segments.warps.clone();
        let mut speeds = segments.speeds.clone();
        let mut scrolls = segments.scrolls.clone();
        let mut fakes = segments.fakes.clone();

        stops.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        delays.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        warps.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        speeds.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        scrolls.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));
        fakes.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Less));

        let song_offset_sec = song_offset_sec + segments.beat0_offset_adjust;

        let mut beat_to_time = Vec::with_capacity(parsed_bpms.len());
        let mut current_time = 0.0;
        let mut last_beat = 0.0;
        let mut last_bpm = parsed_bpms[0].1;
        let mut max_bpm = 0.0;

        for &(beat, bpm) in &parsed_bpms {
            if beat > last_beat && last_bpm > 0.0 {
                current_time += (beat - last_beat) * (60.0 / last_bpm);
            }
            beat_to_time.push(BeatTimePoint {
                beat,
                time_sec: song_offset_sec + current_time,
                bpm,
            });
            if bpm.is_finite() && bpm > max_bpm {
                max_bpm = bpm;
            }
            last_beat = beat;
            last_bpm = bpm;
        }

        let mut timing_with_stops = Self {
            row_to_beat: Arc::new(vec![]),
            beat_to_time: Arc::new(beat_to_time),
            stops,
            delays,
            warps,
            speeds,
            scrolls,
            fakes,
            speed_runtime: Vec::new(),
            scroll_prefix: Vec::new(),
            global_offset_sec,
            max_bpm,
        };

        let re_beat_to_time: Vec<_> = timing_with_stops
            .beat_to_time
            .iter()
            .map(|point| {
                let mut new_point = *point;
                new_point.time_sec = timing_with_stops.get_time_for_beat_internal(point.beat);
                new_point
            })
            .collect();
        timing_with_stops.beat_to_time = Arc::new(re_beat_to_time);

        if !timing_with_stops.speeds.is_empty() {
            let mut runtime = Vec::with_capacity(timing_with_stops.speeds.len());
            let mut prev_ratio = 1.0_f32;
            for seg in &timing_with_stops.speeds {
                let start_time = timing_with_stops.get_time_for_beat(seg.beat);
                let end_time = if seg.delay <= 0.0 {
                    start_time
                } else if seg.unit == SpeedUnit::Seconds {
                    start_time + seg.delay
                } else {
                    timing_with_stops.get_time_for_beat(seg.beat + seg.delay)
                };
                runtime.push(SpeedRuntime {
                    start_time,
                    end_time,
                    prev_ratio,
                });
                prev_ratio = seg.ratio;
            }
            timing_with_stops.speed_runtime = runtime;
        }

        if !timing_with_stops.scrolls.is_empty() {
            let mut prefixes = Vec::with_capacity(timing_with_stops.scrolls.len());
            let mut cum_displayed = 0.0_f32;
            let mut last_real_beat = 0.0_f32;
            let mut last_ratio = 1.0_f32;
            for seg in &timing_with_stops.scrolls {
                cum_displayed += (seg.beat - last_real_beat) * last_ratio;
                prefixes.push(ScrollPrefix {
                    beat: seg.beat,
                    cum_displayed,
                    ratio: seg.ratio,
                });
                last_real_beat = seg.beat;
                last_ratio = seg.ratio;
            }
            timing_with_stops.scroll_prefix = prefixes;
        }

        let row_to_beat = row_to_beat.to_vec();
        info!("TimingData processed {} note rows.", row_to_beat.len());
        timing_with_stops.row_to_beat = Arc::new(row_to_beat);

        timing_with_stops
    }

    #[inline(always)]
    pub fn is_fake_at_beat(&self, beat: f32) -> bool {
        if self.fakes.is_empty() {
            return false;
        }
        // Binary search for last segment starting at or before beat
        let idx = self.fakes.partition_point(|seg| seg.beat <= beat);
        if idx == 0 {
            return false;
        }
        let seg = self.fakes[idx - 1];
        beat >= seg.beat && beat < seg.beat + seg.length
    }

    #[inline(always)]
    pub fn is_warp_at_beat(&self, beat: f32) -> bool {
        if self.warps.is_empty() {
            return false;
        }
        // warps represent a range [beat, beat+length) of non-judgable rows
        let idx = self.warps.partition_point(|seg| seg.beat <= beat);
        if idx == 0 {
            return false;
        }
        let seg = self.warps[idx - 1];
        // Ignore degenerate or negative-length warps
        if !(seg.length.is_finite() && seg.length > 0.0) {
            return false;
        }
        beat >= seg.beat && beat < seg.beat + seg.length
    }

    #[inline(always)]
    pub fn is_judgable_at_beat(&self, beat: f32) -> bool {
        !self.is_warp_at_beat(beat) && !self.is_fake_at_beat(beat)
    }

    pub fn get_beat_for_row(&self, row_index: usize) -> Option<f32> {
        self.row_to_beat.get(row_index).copied()
    }

    pub fn get_row_for_beat(&self, target_beat: f32) -> Option<usize> {
        let rows = self.row_to_beat.as_ref();
        if rows.is_empty() {
            return None;
        }

        let idx = match rows
            .binary_search_by(|beat| beat.partial_cmp(&target_beat).unwrap_or(Ordering::Less))
        {
            Ok(i) => i,
            Err(i) => {
                if i == 0 {
                    0
                } else if i >= rows.len() {
                    rows.len() - 1
                } else {
                    let lower = rows[i - 1];
                    let upper = rows[i];
                    if (target_beat - lower).abs() <= (upper - target_beat).abs() {
                        i - 1
                    } else {
                        i
                    }
                }
            }
        };

        Some(idx)
    }

    pub fn get_beat_info_from_time(&self, target_time_sec: f32) -> BeatInfo {
        let mut args = GetBeatArgs::default();
        args.elapsed_time = target_time_sec + self.global_offset_sec;

        let mut start = GetBeatStarts::default();
        start.last_time = -self.beat0_offset_seconds() - self.beat0_group_offset_seconds();

        self.get_beat_internal(&mut start, &mut args, u32::MAX as usize);

        BeatInfo {
            beat: args.beat,
            is_in_freeze: args.freeze_out,
            is_in_delay: args.delay_out,
        }
    }

    pub fn get_beat_info_from_time_cached(
        &self,
        target_time_sec: f32,
        cache: &mut BeatInfoCache,
    ) -> BeatInfo {
        let elapsed_time = target_time_sec + self.global_offset_sec;
        if cache.global_offset_sec != self.global_offset_sec
            || !cache.last_elapsed_time.is_finite()
            || elapsed_time < cache.last_elapsed_time
        {
            cache.reset(self);
        }

        let mut args = GetBeatArgs::default();
        args.elapsed_time = elapsed_time;
        self.get_beat_internal(&mut cache.start, &mut args, u32::MAX as usize);
        cache.last_elapsed_time = elapsed_time;

        BeatInfo {
            beat: args.beat,
            is_in_freeze: args.freeze_out,
            is_in_delay: args.delay_out,
        }
    }

    pub fn get_beat_for_time(&self, target_time_sec: f32) -> f32 {
        self.get_beat_info_from_time(target_time_sec).beat
    }

    fn get_bpm_point_index_for_beat(&self, target_beat: f32) -> usize {
        let points = &self.beat_to_time;
        if points.is_empty() {
            return 0;
        }

        match points.binary_search_by(|p| {
            p.beat
                .partial_cmp(&target_beat)
                .unwrap_or(std::cmp::Ordering::Less)
        }) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    pub fn get_time_for_beat(&self, target_beat: f32) -> f32 {
        self.get_time_for_beat_internal(target_beat) - self.global_offset_sec
    }

    fn get_time_for_beat_internal(&self, target_beat: f32) -> f32 {
        let mut starts = GetBeatStarts::default();
        starts.last_time = -self.beat0_offset_seconds() - self.beat0_group_offset_seconds();
        self.get_elapsed_time_internal(&mut starts, target_beat)
    }

    pub fn get_bpm_for_beat(&self, target_beat: f32) -> f32 {
        let points = &self.beat_to_time;
        if points.is_empty() {
            return 60.0;
        } // Fallback BPM
        let point_idx = self.get_bpm_point_index_for_beat(target_beat);
        points[point_idx].bpm
    }

    pub fn get_capped_max_bpm(&self, cap: Option<f32>) -> f32 {
        let mut max_bpm = self.max_bpm.max(0.0);
        if max_bpm == 0.0 {
            max_bpm = self
                .beat_to_time
                .iter()
                .map(|point| point.bpm)
                .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
                .fold(0.0, f32::max);
        }

        if let Some(cap_value) = cap
            && cap_value > 0.0
        {
            max_bpm = max_bpm.min(cap_value);
        }

        if max_bpm > 0.0 { max_bpm } else { 60.0 }
    }
}

impl TimingData {
    fn beat0_offset_seconds(&self) -> f32 {
        self.beat_to_time.first().map_or(0.0, |p| p.time_sec)
    }
    fn beat0_group_offset_seconds(&self) -> f32 {
        self.global_offset_sec
    }

    /// Update the global offset used for time⇄beat conversion, mirroring
    /// ITGmania semantics while keeping precomputed data consistent.
    #[inline(always)]
    pub fn set_global_offset_seconds(&mut self, new_offset: f32) {
        let old = self.global_offset_sec;
        if (old - new_offset).abs() < f32::EPSILON {
            return;
        }
        // Adjust beat0 offset so that beat→time mapping shifts by (old - new)
        // instead of being recomputed from raw timing data.
        if let Some(first) = Arc::make_mut(&mut self.beat_to_time).first_mut() {
            first.time_sec += old - new_offset;
        }
        self.global_offset_sec = new_offset;

        // Rebuild speed_runtime, since its start/end times are in song time.
        if !self.speeds.is_empty() {
            let mut runtime = Vec::with_capacity(self.speeds.len());
            let mut prev_ratio = 1.0_f32;
            for seg in &self.speeds {
                let start_time = self.get_time_for_beat(seg.beat);
                let end_time = if seg.delay <= 0.0 {
                    start_time
                } else if seg.unit == SpeedUnit::Seconds {
                    start_time + seg.delay
                } else {
                    self.get_time_for_beat(seg.beat + seg.delay)
                };
                runtime.push(SpeedRuntime {
                    start_time,
                    end_time,
                    prev_ratio,
                });
                prev_ratio = seg.ratio;
            }
            self.speed_runtime = runtime;
        }
        // scroll_prefix depends only on beats/ratios, not absolute time.
    }

    fn get_elapsed_time_internal(&self, starts: &mut GetBeatStarts, beat: f32) -> f32 {
        let mut start = *starts;
        self.get_elapsed_time_internal_mut(&mut start, beat, u32::MAX as usize);
        start.last_time
    }

    fn get_beat_internal(
        &self,
        start: &mut GetBeatStarts,
        args: &mut GetBeatArgs,
        max_segment: usize,
    ) {
        let bpms = &self.beat_to_time;
        let warps = &self.warps;
        let stops = &self.stops;
        let delays = &self.delays;

        let mut curr_segment = start.bpm_idx + start.warp_idx + start.stop_idx + start.delay_idx;
        let mut bps = self.get_bpm_for_beat(note_row_to_beat(start.last_row)) / 60.0;
        while curr_segment < max_segment {
            let mut event_row = i32::MAX;
            let mut event_type = TimingEvent::NotFound;
            find_event(
                &mut event_row,
                &mut event_type,
                *start,
                0.0,
                false,
                bpms,
                warps,
                stops,
                delays,
            );
            if event_type == TimingEvent::NotFound {
                break;
            }
            let time_to_next_event = if start.is_warping {
                0.0
            } else {
                note_row_to_beat(event_row - start.last_row) / bps
            };
            let next_event_time = start.last_time + time_to_next_event;
            if args.elapsed_time < next_event_time {
                break;
            }
            start.last_time = next_event_time;

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bps = bpms[start.bpm_idx].bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay | TimingEvent::StopDelay => {
                    let delay = delays[start.delay_idx];
                    if args.elapsed_time < start.last_time + delay.duration {
                        args.delay_out = true;
                        args.beat = delay.beat;
                        args.bps_out = bps;
                        start.last_row = event_row;
                        return;
                    }
                    start.last_time += delay.duration;
                    start.delay_idx += 1;
                    curr_segment += 1;
                    if event_type == TimingEvent::Delay {
                        start.last_row = event_row;
                        continue;
                    }
                }
                TimingEvent::Stop => {
                    let stop = stops[start.stop_idx];
                    if args.elapsed_time < start.last_time + stop.duration {
                        args.freeze_out = true;
                        args.beat = stop.beat;
                        args.bps_out = bps;
                        start.last_row = event_row;
                        return;
                    }
                    start.last_time += stop.duration;
                    start.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Warp => {
                    start.is_warping = true;
                    let warp = warps[start.warp_idx];
                    let warp_sum = warp.length + warp.beat;
                    if warp_sum > start.warp_destination {
                        start.warp_destination = warp_sum;
                    }
                    args.warp_begin_out = event_row;
                    args.warp_dest_out = start.warp_destination;
                    start.warp_idx += 1;
                    curr_segment += 1;
                }
                _ => {}
            }
            start.last_row = event_row;
        }
        if args.elapsed_time == f32::MAX {
            args.elapsed_time = start.last_time;
        }
        args.beat = note_row_to_beat(start.last_row) + (args.elapsed_time - start.last_time) * bps;
        args.bps_out = bps;
    }

    fn get_elapsed_time_internal_mut(
        &self,
        start: &mut GetBeatStarts,
        beat: f32,
        max_segment: usize,
    ) {
        let bpms = &self.beat_to_time;
        let warps = &self.warps;
        let stops = &self.stops;
        let delays = &self.delays;

        let mut curr_segment = start.bpm_idx + start.warp_idx + start.stop_idx + start.delay_idx;
        let mut bps = self.get_bpm_for_beat(note_row_to_beat(start.last_row)) / 60.0;
        let find_marker = beat < f32::MAX;

        while curr_segment < max_segment {
            let mut event_row = i32::MAX;
            let mut event_type = TimingEvent::NotFound;
            find_event(
                &mut event_row,
                &mut event_type,
                *start,
                beat,
                find_marker,
                bpms,
                warps,
                stops,
                delays,
            );
            if event_type == TimingEvent::NotFound {
                break;
            }
            let time_to_next_event = if start.is_warping {
                0.0
            } else {
                note_row_to_beat(event_row - start.last_row) / bps
            };
            start.last_time += time_to_next_event;

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bps = bpms[start.bpm_idx].bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Stop | TimingEvent::StopDelay => {
                    start.last_time += stops[start.stop_idx].duration;
                    start.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay => {
                    start.last_time += delays[start.delay_idx].duration;
                    start.delay_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Marker => return,
                TimingEvent::Warp => {
                    start.is_warping = true;
                    let warp = warps[start.warp_idx];
                    let warp_sum = warp.length + warp.beat;
                    if warp_sum > start.warp_destination {
                        start.warp_destination = warp_sum;
                    }
                    start.warp_idx += 1;
                    curr_segment += 1;
                }
                _ => {}
            }
            start.last_row = event_row;
        }
    }

    pub fn get_displayed_beat(&self, beat: f32) -> f32 {
        if self.scroll_prefix.is_empty() {
            return beat;
        }
        // If before first scroll segment, base ratio is 1.0 from 0.0
        if beat < self.scroll_prefix[0].beat {
            return beat;
        }
        let idx = self.scroll_prefix.partition_point(|p| p.beat <= beat);
        let i = idx.saturating_sub(1);
        let p = self.scroll_prefix[i];
        p.cum_displayed + (beat - p.beat) * p.ratio
    }

    pub fn get_speed_multiplier(&self, beat: f32, time: f32) -> f32 {
        if self.speeds.is_empty() {
            return 1.0;
        }
        let segment_index = self.get_speed_segment_index_at_beat(beat);
        if segment_index < 0 {
            return 1.0;
        }
        let i = segment_index as usize;
        let seg = self.speeds[i];
        let rt = self.speed_runtime.get(i).copied().unwrap_or(SpeedRuntime {
            start_time: self.get_time_for_beat(seg.beat),
            end_time: if seg.unit == SpeedUnit::Seconds {
                self.get_time_for_beat(seg.beat) + seg.delay
            } else {
                self.get_time_for_beat(seg.beat + seg.delay)
            },
            prev_ratio: if i > 0 { self.speeds[i - 1].ratio } else { 1.0 },
        });

        if time >= rt.end_time || seg.delay <= 0.0 {
            return seg.ratio;
        }
        if time < rt.start_time {
            return rt.prev_ratio;
        }
        let progress = (time - rt.start_time) / (rt.end_time - rt.start_time);
        rt.prev_ratio + (seg.ratio - rt.prev_ratio) * progress
    }

    fn get_speed_segment_index_at_beat(&self, beat: f32) -> isize {
        if self.speeds.is_empty() {
            return -1;
        }
        let pos = self.speeds.partition_point(|seg| seg.beat <= beat);

        if pos == 0 { -1 } else { (pos - 1) as isize }
    }
}

fn find_event(
    event_row: &mut i32,
    event_type: &mut TimingEvent,
    start: GetBeatStarts,
    beat: f32,
    find_marker: bool,
    bpms: &Arc<Vec<BeatTimePoint>>,
    warps: &[WarpSegment],
    stops: &[StopSegment],
    delays: &[DelaySegment],
) {
    if start.is_warping && beat_to_note_row(start.warp_destination) < *event_row {
        *event_row = beat_to_note_row(start.warp_destination);
        *event_type = TimingEvent::WarpDest;
    }
    if start.bpm_idx < bpms.len() && beat_to_note_row(bpms[start.bpm_idx].beat) < *event_row {
        *event_row = beat_to_note_row(bpms[start.bpm_idx].beat);
        *event_type = TimingEvent::Bpm;
    }
    if start.delay_idx < delays.len() && beat_to_note_row(delays[start.delay_idx].beat) < *event_row
    {
        *event_row = beat_to_note_row(delays[start.delay_idx].beat);
        *event_type = TimingEvent::Delay;
    }
    if find_marker && beat_to_note_row(beat) < *event_row {
        *event_row = beat_to_note_row(beat);
        *event_type = TimingEvent::Marker;
    }
    if start.stop_idx < stops.len() && beat_to_note_row(stops[start.stop_idx].beat) < *event_row {
        let tmp_row = *event_row;
        *event_row = beat_to_note_row(stops[start.stop_idx].beat);
        *event_type = if tmp_row == *event_row {
            TimingEvent::StopDelay
        } else {
            TimingEvent::Stop
        };
    }
    if start.warp_idx < warps.len() && beat_to_note_row(warps[start.warp_idx].beat) < *event_row {
        *event_row = beat_to_note_row(warps[start.warp_idx].beat);
        *event_type = TimingEvent::Warp;
    }
}

// ------------------ Timing Stats + Graph Prep (Histogram & Scatter) ------------------

#[derive(Copy, Clone, Debug, Default)]
pub struct TimingStats {
    pub mean_abs_ms: f32,
    pub mean_ms: f32,
    pub stddev_ms: f32,
    pub max_abs_ms: f32,
}

#[inline(always)]
pub fn compute_note_timing_stats(notes: &[Note]) -> TimingStats {
    // First pass: accumulate sums and maxima over non-miss judgments
    let mut sum_abs = 0.0_f32;
    let mut sum_signed = 0.0_f32;
    let mut max_abs = 0.0_f32;
    let mut count: usize = 0;

    for n in notes {
        if let Some(j) = &n.result
            && j.grade != JudgeGrade::Miss
        {
            let e = j.time_error_ms;
            let a = e.abs();
            sum_abs += a;
            sum_signed += e;
            if a > max_abs {
                max_abs = a;
            }
            count += 1;
        }
    }

    if count == 0 {
        return TimingStats::default();
    }

    let mean_ms = sum_signed / (count as f32);
    let mean_abs_ms = sum_abs / (count as f32);

    // Second pass: sample standard deviation of signed offsets
    let stddev_ms = if count > 1 {
        let mut sum_diff_sq = 0.0_f32;
        for n in notes {
            if let Some(j) = &n.result
                && j.grade != JudgeGrade::Miss
            {
                let d = j.time_error_ms - mean_ms;
                sum_diff_sq += d * d;
            }
        }
        (sum_diff_sq / ((count as f32) - 1.0)).sqrt()
    } else {
        0.0
    };

    TimingStats {
        mean_abs_ms,
        mean_ms,
        stddev_ms,
        max_abs_ms: max_abs,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ScatterPoint {
    pub time_sec: f32,
    pub offset_ms: Option<f32>, // None for Miss
}

#[derive(Clone, Debug, Default)]
pub struct HistogramMs {
    pub bins: Vec<(i32, u32)>,     // raw counts (bin_ms, count), sorted by bin
    pub smoothed: Vec<(i32, f32)>, // Gaussian-smoothed counts (bin_ms, value)
    pub max_count: u32,            // peak of raw counts
    pub worst_observed_ms: f32,    // max |offset| actually observed
    pub worst_window_ms: f32,      // for scaling (-worst..+worst)
}

const HIST_BIN_MS: f32 = 1.0; // 1ms bins, like Simply Love using 0.001s
// Gaussian-like kernel used by Simply Love to soften the histogram
const GAUSS7: [f32; 7] = [0.045, 0.090, 0.180, 0.370, 0.180, 0.090, 0.045];

#[inline(always)]
pub fn build_scatter_points(notes: &[Note], note_time_cache: &[f32]) -> Vec<ScatterPoint> {
    let mut out = Vec::with_capacity(notes.len());
    for (idx, n) in notes.iter().enumerate() {
        if matches!(n.note_type, NoteType::Mine) {
            continue;
        }
        if n.is_fake {
            continue;
        }
        let t = note_time_cache.get(idx).copied().unwrap_or(0.0);
        let offset_ms = match n.result.as_ref() {
            Some(j) => {
                if j.grade == JudgeGrade::Miss {
                    None
                } else {
                    Some(j.time_error_ms)
                }
            }
            None => continue, // do not include unjudged notes
        };
        out.push(ScatterPoint {
            time_sec: t,
            offset_ms,
        });
    }
    out
}

#[inline(always)]
fn bin_index_ms(v_ms: f32) -> i32 {
    // Mirror Simply Love behavior: floor to 1ms steps, with negative going more negative
    (v_ms / HIST_BIN_MS).floor() as i32
}

#[inline(always)]
pub fn build_histogram_ms(notes: &[Note]) -> HistogramMs {
    use std::collections::HashMap;
    let mut counts: HashMap<i32, u32> = HashMap::new();
    let mut max_count: u32 = 0;
    let mut max_abs: f32 = 0.0;
    // Determine worst timing window seen (at least W3 per Simply Love histogram)
    let mut worst_window_index = 3; // 1=W1..5=W5
    let mut worst_observed_bin_abs: i32 = 0;

    for n in notes {
        let Some(j) = n.result.as_ref() else {
            continue;
        };
        if j.grade == JudgeGrade::Miss {
            continue;
        }
        if matches!(n.note_type, NoteType::Mine) {
            continue;
        }
        if n.is_fake {
            continue;
        }
        let e = j.time_error_ms;
        let b = bin_index_ms(e);
        let c = counts.entry(b).or_insert(0);
        *c = c.saturating_add(1);
        if *c > max_count {
            max_count = *c;
        }
        let a = e.abs();
        if a > max_abs {
            max_abs = a;
        }
        if b.abs() > worst_observed_bin_abs {
            worst_observed_bin_abs = b.abs();
        }

        match j.grade {
            JudgeGrade::WayOff => worst_window_index = worst_window_index.max(5),
            JudgeGrade::Decent => worst_window_index = worst_window_index.max(4),
            JudgeGrade::Great => worst_window_index = worst_window_index.max(3),
            JudgeGrade::Excellent => worst_window_index = worst_window_index.max(2),
            JudgeGrade::Fantastic => worst_window_index = worst_window_index.max(1),
            JudgeGrade::Miss => {}
        }
    }

    let mut bins: Vec<(i32, u32)> = counts.into_iter().collect();
    bins.sort_unstable_by_key(|(bin, _)| *bin);

    let eff = effective_windows_ms();
    let worst_window_ms: f32 = match worst_window_index {
        1 => eff[0],
        2 => eff[1],
        3 => eff[2],
        4 => eff[3],
        _ => eff[4],
    };

    // Build smoothed distribution across the whole timing window range (1ms steps)
    let worst_window_bin = (worst_window_ms / HIST_BIN_MS).round() as i32;
    let mut smoothed: Vec<(i32, f32)> =
        Vec::with_capacity((worst_window_bin * 2 + 1).max(1) as usize);

    // Rebuild a fast lookup for counts
    let mut count_map: HashMap<i32, u32> = HashMap::with_capacity(bins.len());
    for (bin, c) in &bins {
        count_map.insert(*bin, *c);
    }

    for i in -worst_window_bin..=worst_window_bin {
        let mut y = 0.0_f32;
        for (j, w) in GAUSS7.iter().enumerate() {
            let offset = j as i32 - 3; // -3..+3
            let k = (i + offset).clamp(-worst_window_bin, worst_window_bin);
            let c = *count_map.get(&k).unwrap_or(&0) as f32;
            y += c * *w;
        }
        smoothed.push((i, y));
    }

    HistogramMs {
        bins,
        smoothed,
        max_count,
        worst_observed_ms: (worst_observed_bin_abs as f32) * HIST_BIN_MS,
        worst_window_ms: worst_window_ms.max(max_abs),
    }
}

// ----------------------------- FA+ / Window Counts -----------------------------

#[derive(Copy, Clone, Debug, Default)]
pub struct WindowCounts {
    pub w0: u32,
    pub w1: u32,
    pub w2: u32,
    pub w3: u32,
    pub w4: u32,
    pub w5: u32,
    pub miss: u32,
}

#[inline(always)]
pub fn compute_window_counts(notes: &[Note]) -> WindowCounts {
    let mut out = WindowCounts::default();
    if notes.is_empty() {
        return out;
    }

    let mut idx: usize = 0;
    let len = notes.len();

    while idx < len {
        let row_index = notes[idx].row_index;
        let mut row_judgments: Vec<&Judgment> = Vec::new();

        while idx < len && notes[idx].row_index == row_index {
            let note = &notes[idx];
            if !note.is_fake && note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
                if let Some(j) = note.result.as_ref() {
                    row_judgments.push(j);
                }
            }
            idx += 1;
        }

        if row_judgments.is_empty() {
            continue;
        }

        if let Some(j) = judgment::aggregate_row_final_judgment(row_judgments.iter().copied()) {
            match j.grade {
                JudgeGrade::Fantastic => match j.window {
                    Some(TimingWindow::W0) => out.w0 = out.w0.saturating_add(1),
                    _ => out.w1 = out.w1.saturating_add(1),
                },
                JudgeGrade::Excellent => out.w2 = out.w2.saturating_add(1),
                JudgeGrade::Great => out.w3 = out.w3.saturating_add(1),
                JudgeGrade::Decent => out.w4 = out.w4.saturating_add(1),
                JudgeGrade::WayOff => out.w5 = out.w5.saturating_add(1),
                JudgeGrade::Miss => out.miss = out.miss.saturating_add(1),
            }
        }
    }

    out
}
