use crate::game::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use crate::game::note::{Note, NoteType};
use log::debug;
use rssp::streams::StreamSegment;
use rssp::timing as rssp_timing;
use std::cmp::Ordering;
use std::sync::Arc;

// --- ITGMania Parity Constants and Helpers ---
pub const ROWS_PER_BEAT: i32 = 48;
type TimingNs = i64;
const INVALID_TIMING_NS: TimingNs = i64::MIN;

#[inline(always)]
fn timing_ns_from_seconds(seconds: f32) -> TimingNs {
    let nanos = f64::from(seconds) * 1_000_000_000.0;
    nanos.clamp((i64::MIN + 1) as f64, i64::MAX as f64) as TimingNs
}

#[inline(always)]
fn timing_ns_to_seconds(time_ns: TimingNs) -> f32 {
    (time_ns as f64 * 1.0e-9) as f32
}

#[inline(always)]
fn timing_ns_delta_seconds(lhs: TimingNs, rhs: TimingNs) -> f32 {
    ((lhs as i128 - rhs as i128) as f64 * 1.0e-9) as f32
}

#[inline(always)]
fn timing_ns_add_seconds(time_ns: TimingNs, delta_seconds: f32) -> TimingNs {
    time_ns.saturating_add(timing_ns_from_seconds(delta_seconds))
}

// ------------------ Unified Timing Windows (Gameplay + Visuals) ------------------
// All base windows are in seconds.
pub const TIMING_WINDOW_ADD_S: f32 = 0.0015; // +1.5ms padding applied by ITG/SM

// ITG tap windows (seconds, exclusive of TIMING_WINDOW_ADD_S).
pub const BASE_W1_S: f32 = 0.0215;
pub const BASE_W2_S: f32 = 0.0430;
pub const BASE_W3_S: f32 = 0.1020;
pub const BASE_W4_S: f32 = 0.1350;
pub const BASE_W5_S: f32 = 0.1800;
// Simply Love sets TimingWindowSecondsMine=0.070 across Casual/ITG/FA+.
pub const BASE_MINE_S: f32 = 0.0700;

// FA+ inner Fantastic window (W0) is defined using Simply Love's FA+ W1 timing.
// See SL.Preferences["FA+"].TimingWindowSecondsW1 in SL_Init.lua.
pub const BASE_FA_PLUS_W0_S: f32 = 0.0135;
pub const FA_PLUS_W0_MS: f32 = (BASE_FA_PLUS_W0_S + TIMING_WINDOW_ADD_S) * 1000.0;

// Arrow Cloud: 10ms FA+ support for "SmallerWhite".
// See Arrow Cloud's IsW010Judgment() helper (base 8.5ms + 1.5ms add == 10ms).
pub const BASE_FA_PLUS_W010_S: f32 = 0.0085;
pub const FA_PLUS_W010_MS: f32 = (BASE_FA_PLUS_W010_S + TIMING_WINDOW_ADD_S) * 1000.0;

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
        Self {
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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct TimingProfileNs {
    pub windows_ns: [i64; 5],
    pub fa_plus_window_ns: Option<i64>,
    pub mine_window_ns: i64,
}

impl TimingProfileNs {
    #[inline(always)]
    pub fn from_profile_scaled(profile: &TimingProfile, seconds_per_second: f32) -> Self {
        #[inline(always)]
        fn scale_window_ns(seconds: f32, seconds_per_second: f32) -> TimingNs {
            let seconds_per_second = if seconds_per_second.is_finite() && seconds_per_second > 0.0 {
                seconds_per_second
            } else {
                1.0
            };
            let nanos = f64::from(seconds) * f64::from(seconds_per_second) * 1_000_000_000.0;
            nanos.round().clamp((i64::MIN + 1) as f64, i64::MAX as f64) as TimingNs
        }

        Self {
            windows_ns: profile
                .windows_s
                .map(|seconds| scale_window_ns(seconds, seconds_per_second)),
            fa_plus_window_ns: profile
                .fa_plus_window_s
                .map(|seconds| scale_window_ns(seconds, seconds_per_second)),
            mine_window_ns: scale_window_ns(profile.mine_window_s, seconds_per_second),
        }
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

#[inline(always)]
pub fn classify_offset_ns_with_disabled_windows(
    offset_ns: i64,
    profile: &TimingProfileNs,
    disabled_windows: &[bool; 5],
) -> Option<(JudgeGrade, TimingWindow)> {
    let abs = i128::from(offset_ns).abs();
    if !disabled_windows[0]
        && let Some(w0) = profile.fa_plus_window_ns
        && abs <= i128::from(w0)
    {
        return Some((JudgeGrade::Fantastic, TimingWindow::W0));
    }

    let checks = [
        (
            disabled_windows[0],
            profile.windows_ns[0],
            JudgeGrade::Fantastic,
            TimingWindow::W1,
        ),
        (
            disabled_windows[1],
            profile.windows_ns[1],
            JudgeGrade::Excellent,
            TimingWindow::W2,
        ),
        (
            disabled_windows[2],
            profile.windows_ns[2],
            JudgeGrade::Great,
            TimingWindow::W3,
        ),
        (
            disabled_windows[3],
            profile.windows_ns[3],
            JudgeGrade::Decent,
            TimingWindow::W4,
        ),
        (
            disabled_windows[4],
            profile.windows_ns[4],
            JudgeGrade::WayOff,
            TimingWindow::W5,
        ),
    ];
    for (disabled, window_ns, grade, window) in checks {
        if !disabled && abs <= i128::from(window_ns) {
            return Some((grade, window));
        }
    }
    None
}

#[inline(always)]
pub fn largest_enabled_tap_window_ns(
    profile: &TimingProfileNs,
    disabled_windows: &[bool; 5],
) -> Option<i64> {
    let windows = profile.windows_ns;
    let ordered = [
        (disabled_windows[4], windows[4]),
        (disabled_windows[3], windows[3]),
        (disabled_windows[2], windows[2]),
        (disabled_windows[1], windows[1]),
        (disabled_windows[0], windows[0]),
    ];
    for (disabled, window_ns) in ordered {
        if !disabled {
            return Some(window_ns);
        }
    }
    None
}

#[inline(always)]
pub fn note_row_to_beat(row: i32) -> f32 {
    row as f32 / ROWS_PER_BEAT as f32
}

#[inline(always)]
pub fn beat_to_note_row(beat: f32) -> i32 {
    (beat * ROWS_PER_BEAT as f32).round() as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy)]
pub struct TimeSignatureSegment {
    pub beat: f32,
    pub numerator: i32,
    pub denominator: i32,
}

#[derive(Debug, Clone)]
pub struct TimingSegments {
    pub beat0_offset_adjust: f32,
    pub bpms: Vec<(f32, f32)>,
    pub stops: Vec<StopSegment>,
    pub delays: Vec<DelaySegment>,
    pub warps: Vec<WarpSegment>,
    pub speeds: Vec<SpeedSegment>,
    pub scrolls: Vec<ScrollSegment>,
    pub fakes: Vec<FakeSegment>,
    pub time_signatures: Vec<TimeSignatureSegment>,
}

impl Default for TimingSegments {
    fn default() -> Self {
        Self {
            beat0_offset_adjust: 0.0,
            bpms: Vec::new(),
            stops: Vec::new(),
            delays: Vec::new(),
            warps: Vec::new(),
            speeds: Vec::new(),
            scrolls: Vec::new(),
            fakes: Vec::new(),
            time_signatures: default_time_signatures(),
        }
    }
}

pub fn default_time_signature() -> TimeSignatureSegment {
    TimeSignatureSegment {
        beat: 0.0,
        numerator: 4,
        denominator: 4,
    }
}

pub fn default_time_signatures() -> Vec<TimeSignatureSegment> {
    vec![default_time_signature()]
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
            time_signatures: default_time_signatures(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SpeedRuntime {
    start_time_ns: TimingNs,
    end_time_ns: TimingNs,
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
    global_offset_ns: TimingNs,
    max_bpm: f32,
}

#[derive(Debug, Clone, Default, Copy)]
struct BeatTimePoint {
    beat: f32,
    time_ns: TimingNs,
    bpm: f32,
}

#[derive(Debug, Clone, Copy)]
struct GetBeatStarts {
    bpm_idx: usize,
    stop_idx: usize,
    delay_idx: usize,
    warp_idx: usize,
    last_row: i32,
    last_time_ns: TimingNs,
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
            last_time_ns: 0,
            warp_destination: 0.0,
            is_warping: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BeatInfoCache {
    start: GetBeatStarts,
    last_elapsed_time_ns: TimingNs,
    global_offset_ns: TimingNs,
}

impl BeatInfoCache {
    pub fn new(timing: &TimingData) -> Self {
        let mut cache = Self {
            start: GetBeatStarts::default(),
            last_elapsed_time_ns: INVALID_TIMING_NS,
            global_offset_ns: timing.global_offset_ns,
        };
        cache.start.last_time_ns = timing.beat_start_time_ns();
        cache
    }

    pub fn reset(&mut self, timing: &TimingData) {
        self.start = GetBeatStarts::default();
        self.start.last_time_ns = timing.beat_start_time_ns();
        self.last_elapsed_time_ns = INVALID_TIMING_NS;
        self.global_offset_ns = timing.global_offset_ns;
    }
}

#[derive(Debug, Clone, Copy)]
struct GetBeatArgs {
    elapsed_time_ns: TimingNs,
    beat: f32,
    bps_out: f32,
    warp_dest_out: f32,
    warp_begin_out: i32,
    freeze_out: bool,
    delay_out: bool,
}

impl Default for GetBeatArgs {
    fn default() -> Self {
        Self {
            elapsed_time_ns: INVALID_TIMING_NS,
            beat: 0.0,
            bps_out: 0.0,
            warp_dest_out: 0.0,
            warp_begin_out: 0,
            freeze_out: false,
            delay_out: false,
        }
    }
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
        let song_offset_ns = timing_ns_from_seconds(song_offset_sec);
        let global_offset_ns = timing_ns_from_seconds(global_offset_sec);

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
                time_ns: timing_ns_add_seconds(song_offset_ns, current_time),
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
            global_offset_ns,
            max_bpm,
        };

        let re_beat_to_time: Vec<_> = timing_with_stops
            .beat_to_time
            .iter()
            .map(|point| {
                let mut new_point = *point;
                new_point.time_ns = timing_with_stops.get_time_for_beat_internal_ns(point.beat);
                new_point
            })
            .collect();
        timing_with_stops.beat_to_time = Arc::new(re_beat_to_time);

        timing_with_stops.rebuild_speed_runtime();

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
        debug!("TimingData processed {} note rows.", row_to_beat.len());
        timing_with_stops.row_to_beat = Arc::new(row_to_beat);

        timing_with_stops
    }

    #[inline(always)]
    pub fn is_fake_at_beat(&self, beat: f32) -> bool {
        if self.fakes.is_empty() {
            return false;
        }
        let row = beat_to_note_row(beat);
        let beat_row = note_row_to_beat(row);
        // Mirror ITGmania row semantics by quantizing the query beat first.
        let idx = self
            .fakes
            .partition_point(|seg| beat_to_note_row(seg.beat) <= row);
        if idx == 0 {
            return false;
        }
        let seg = self.fakes[idx - 1];
        beat_row >= seg.beat && beat_row < seg.beat + seg.length
    }

    #[inline(always)]
    fn has_stop_or_delay_at_row(&self, row: i32) -> bool {
        self.stops.iter().any(|seg| {
            beat_to_note_row(seg.beat) == row && seg.duration.is_finite() && seg.duration != 0.0
        }) || self.delays.iter().any(|seg| {
            beat_to_note_row(seg.beat) == row && seg.duration.is_finite() && seg.duration != 0.0
        })
    }

    #[inline(always)]
    pub fn is_warp_at_beat(&self, beat: f32) -> bool {
        if self.warps.is_empty() {
            return false;
        }
        let row = beat_to_note_row(beat);
        let beat_row = note_row_to_beat(row);
        let idx = self
            .warps
            .partition_point(|seg| beat_to_note_row(seg.beat) <= row);
        if idx == 0 {
            return false;
        }
        let seg = self.warps[idx - 1];
        // Ignore degenerate or negative-length warps
        if !(seg.length.is_finite() && seg.length > 0.0) {
            return false;
        }
        if !(seg.beat <= beat_row && beat_row < seg.beat + seg.length) {
            return false;
        }

        // ITGmania allows rows with an explicit stop/delay on the warp start row
        // to remain judgable, enabling stop+warp patterns like NULCTRL.
        if self.stops.is_empty() && self.delays.is_empty() {
            return true;
        }

        !self.has_stop_or_delay_at_row(row)
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

    #[inline(always)]
    pub fn cutoff_row_for_note_row(&self, cutoff_note_row: i32) -> usize {
        self.row_to_beat
            .partition_point(|beat| beat_to_note_row(*beat) < cutoff_note_row)
    }

    pub fn get_beat_info_from_time(&self, target_time_sec: f32) -> BeatInfo {
        self.get_beat_info_from_time_ns(timing_ns_from_seconds(target_time_sec))
    }

    pub fn get_beat_info_from_time_ns(&self, target_time_ns: i64) -> BeatInfo {
        let mut args = GetBeatArgs::default();
        args.elapsed_time_ns = target_time_ns.saturating_add(self.global_offset_ns);

        let mut start = GetBeatStarts::default();
        start.last_time_ns = self.beat_start_time_ns();

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
        self.get_beat_info_from_time_ns_cached(timing_ns_from_seconds(target_time_sec), cache)
    }

    pub fn get_beat_info_from_time_ns_cached(
        &self,
        target_time_ns: i64,
        cache: &mut BeatInfoCache,
    ) -> BeatInfo {
        let elapsed_time_ns = target_time_ns.saturating_add(self.global_offset_ns);
        if cache.global_offset_ns != self.global_offset_ns
            || cache.last_elapsed_time_ns == INVALID_TIMING_NS
            || elapsed_time_ns < cache.last_elapsed_time_ns
        {
            cache.reset(self);
        }

        let mut args = GetBeatArgs::default();
        args.elapsed_time_ns = elapsed_time_ns;
        self.get_beat_internal(&mut cache.start, &mut args, u32::MAX as usize);
        cache.last_elapsed_time_ns = elapsed_time_ns;

        BeatInfo {
            beat: args.beat,
            is_in_freeze: args.freeze_out,
            is_in_delay: args.delay_out,
        }
    }

    pub fn get_beat_for_time(&self, target_time_sec: f32) -> f32 {
        self.get_beat_for_time_ns(timing_ns_from_seconds(target_time_sec))
    }

    pub fn get_beat_for_time_ns(&self, target_time_ns: i64) -> f32 {
        self.get_beat_info_from_time_ns(target_time_ns).beat
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
        timing_ns_to_seconds(self.get_time_for_beat_ns(target_beat))
    }

    pub fn get_time_for_beat_ns(&self, target_beat: f32) -> i64 {
        self.get_time_for_beat_internal_ns(target_beat)
            .saturating_sub(self.global_offset_ns)
    }

    fn get_time_for_beat_internal_ns(&self, target_beat: f32) -> TimingNs {
        let mut starts = GetBeatStarts::default();
        starts.last_time_ns = self.beat_start_time_ns();
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

    #[inline(always)]
    pub fn first_bpm(&self) -> f32 {
        self.beat_to_time.first().map_or(60.0, |p| p.bpm)
    }

    #[inline(always)]
    pub fn has_bpm_changes(&self) -> bool {
        self.beat_to_time.len() > 1
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
    fn beat0_offset_ns(&self) -> TimingNs {
        self.beat_to_time.first().map_or(0, |p| p.time_ns)
    }

    fn beat_start_time_ns(&self) -> TimingNs {
        self.beat0_offset_ns()
            .saturating_neg()
            .saturating_sub(self.global_offset_ns)
    }

    fn rebuild_speed_runtime(&mut self) {
        if self.speeds.is_empty() {
            self.speed_runtime.clear();
            return;
        }

        let mut runtime = Vec::with_capacity(self.speeds.len());
        let mut prev_ratio = 1.0_f32;
        for seg in &self.speeds {
            let start_time_ns = self.get_time_for_beat_ns(seg.beat);
            let end_time_ns = if seg.delay <= 0.0 {
                start_time_ns
            } else if seg.unit == SpeedUnit::Seconds {
                timing_ns_add_seconds(start_time_ns, seg.delay)
            } else {
                self.get_time_for_beat_ns(seg.beat + seg.delay)
            };
            runtime.push(SpeedRuntime {
                start_time_ns,
                end_time_ns,
                prev_ratio,
            });
            prev_ratio = seg.ratio;
        }
        self.speed_runtime = runtime;
    }

    /// Update the global offset used for time⇄beat conversion, mirroring
    /// `ITGmania` semantics while keeping precomputed data consistent.
    #[inline(always)]
    pub fn set_global_offset_seconds(&mut self, new_offset: f32) {
        let old = self.global_offset_sec;
        if (old - new_offset).abs() < f32::EPSILON {
            return;
        }
        let new_offset_ns = timing_ns_from_seconds(new_offset);
        // Adjust beat0 offset so that beat→time mapping shifts by (old - new)
        // instead of being recomputed from raw timing data.
        if let Some(first) = Arc::make_mut(&mut self.beat_to_time).first_mut() {
            first.time_ns = first
                .time_ns
                .saturating_add(self.global_offset_ns.saturating_sub(new_offset_ns));
        }
        self.global_offset_sec = new_offset;
        self.global_offset_ns = new_offset_ns;
        self.rebuild_speed_runtime();
        // scroll_prefix depends only on beats/ratios, not absolute time.
    }

    /// Shift song timing by adjusting beat0 offset in seconds.
    /// Positive `delta_seconds` moves note times earlier.
    #[inline(always)]
    pub fn shift_song_offset_seconds(&mut self, delta_seconds: f32) {
        if delta_seconds.abs() < f32::EPSILON {
            return;
        }
        if let Some(first) = Arc::make_mut(&mut self.beat_to_time).first_mut() {
            first.time_ns = timing_ns_add_seconds(first.time_ns, delta_seconds);
        }
        self.rebuild_speed_runtime();
    }

    fn get_elapsed_time_internal(&self, starts: &mut GetBeatStarts, beat: f32) -> TimingNs {
        let mut start = *starts;
        self.get_elapsed_time_internal_mut(&mut start, beat, u32::MAX as usize);
        start.last_time_ns
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
            let time_to_next_event_ns = if start.is_warping {
                0
            } else {
                timing_ns_from_seconds(note_row_to_beat(event_row - start.last_row) / bps)
            };
            let next_event_time_ns = start.last_time_ns.saturating_add(time_to_next_event_ns);
            if args.elapsed_time_ns < next_event_time_ns {
                break;
            }
            start.last_time_ns = next_event_time_ns;

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bps = bpms[start.bpm_idx].bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay | TimingEvent::StopDelay => {
                    let delay = delays[start.delay_idx];
                    let delay_end_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(delay.duration));
                    if args.elapsed_time_ns < delay_end_ns {
                        args.delay_out = true;
                        args.beat = delay.beat;
                        args.bps_out = bps;
                        start.last_row = event_row;
                        return;
                    }
                    start.last_time_ns = delay_end_ns;
                    start.delay_idx += 1;
                    curr_segment += 1;
                    if event_type == TimingEvent::Delay {
                        start.last_row = event_row;
                        continue;
                    }
                }
                TimingEvent::Stop => {
                    let stop = stops[start.stop_idx];
                    let stop_end_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(stop.duration));
                    if args.elapsed_time_ns < stop_end_ns {
                        args.freeze_out = true;
                        args.beat = stop.beat;
                        args.bps_out = bps;
                        start.last_row = event_row;
                        return;
                    }
                    start.last_time_ns = stop_end_ns;
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
        if args.elapsed_time_ns == INVALID_TIMING_NS {
            args.elapsed_time_ns = start.last_time_ns;
        }
        let delta_seconds = timing_ns_delta_seconds(args.elapsed_time_ns, start.last_time_ns);
        args.beat = delta_seconds.mul_add(bps, note_row_to_beat(start.last_row));
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
            let time_to_next_event_ns = if start.is_warping {
                0
            } else {
                timing_ns_from_seconds(note_row_to_beat(event_row - start.last_row) / bps)
            };
            start.last_time_ns = start.last_time_ns.saturating_add(time_to_next_event_ns);

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bps = bpms[start.bpm_idx].bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Stop | TimingEvent::StopDelay => {
                    start.last_time_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(stops[start.stop_idx].duration));
                    start.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay => {
                    start.last_time_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(delays[start.delay_idx].duration));
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
        (beat - p.beat).mul_add(p.ratio, p.cum_displayed)
    }

    pub fn get_speed_multiplier(&self, beat: f32, time: f32) -> f32 {
        self.get_speed_multiplier_ns(beat, timing_ns_from_seconds(time))
    }

    pub fn get_speed_multiplier_ns(&self, beat: f32, time_ns: i64) -> f32 {
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
            start_time_ns: self.get_time_for_beat_ns(seg.beat),
            end_time_ns: if seg.unit == SpeedUnit::Seconds {
                timing_ns_add_seconds(self.get_time_for_beat_ns(seg.beat), seg.delay)
            } else {
                self.get_time_for_beat_ns(seg.beat + seg.delay)
            },
            prev_ratio: if i > 0 { self.speeds[i - 1].ratio } else { 1.0 },
        });

        if time_ns >= rt.end_time_ns || seg.delay <= 0.0 {
            return seg.ratio;
        }
        if time_ns < rt.start_time_ns {
            return rt.prev_ratio;
        }
        let duration_seconds = timing_ns_delta_seconds(rt.end_time_ns, rt.start_time_ns);
        if duration_seconds <= 0.0 {
            return seg.ratio;
        }
        let progress = timing_ns_delta_seconds(time_ns, rt.start_time_ns) / duration_seconds;
        (seg.ratio - rt.prev_ratio).mul_add(progress, rt.prev_ratio)
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
fn for_each_row_final_judgment<F>(notes: &[Note], mut f: F)
where
    F: FnMut(&Judgment),
{
    if notes.is_empty() {
        return;
    }

    let mut idx: usize = 0;
    let len = notes.len();
    let mut row_judgments: Vec<&Judgment> = Vec::with_capacity(8);

    while idx < len {
        let row_index = notes[idx].row_index;
        row_judgments.clear();

        while idx < len && notes[idx].row_index == row_index {
            let note = &notes[idx];
            if !note.is_fake
                && note.can_be_judged
                && !matches!(note.note_type, NoteType::Mine)
                && let Some(j) = note.result.as_ref()
            {
                row_judgments.push(j);
            }
            idx += 1;
        }

        if let Some(j) = judgment::aggregate_row_final_judgment(row_judgments.iter().copied()) {
            f(j);
        }
    }
}

#[inline(always)]
pub fn compute_note_timing_stats(notes: &[Note]) -> TimingStats {
    // First pass: aggregate one final judgment per row, mirroring Simply Love's
    // JudgmentMessage-driven sequential_offsets behavior.
    let mut sum_abs = 0.0_f32;
    let mut sum_signed = 0.0_f32;
    let mut max_abs = 0.0_f32;
    let mut count: usize = 0;

    for_each_row_final_judgment(notes, |j| {
        if j.grade == JudgeGrade::Miss {
            return;
        }
        let e = j.time_error_ms;
        let a = e.abs();
        sum_abs += a;
        sum_signed += e;
        if a > max_abs {
            max_abs = a;
        }
        count += 1;
    });

    if count == 0 {
        return TimingStats::default();
    }

    let mean_ms = sum_signed / (count as f32);
    let mean_abs_ms = sum_abs / (count as f32);

    // Second pass: population standard deviation of signed offsets.
    // This matches ArrowCloud's current website/share-service calculation.
    let mut sum_diff_sq = 0.0_f32;
    for_each_row_final_judgment(notes, |j| {
        if j.grade == JudgeGrade::Miss {
            return;
        }
        let d = j.time_error_ms - mean_ms;
        sum_diff_sq += d * d;
    });
    let stddev_ms = (sum_diff_sq / (count as f32)).sqrt();

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
    // Arrow Cloud-style "direction" code: 1..4 for L/D/U/R, other values for jumps/chords.
    pub direction_code: u8,
    pub is_stream: bool,
    pub is_left_foot: bool,
    pub miss_because_held: bool,
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
fn local_direction_code(note: &Note, col_offset: usize, cols_per_player: usize) -> Option<u8> {
    if note.column < col_offset {
        return None;
    }
    let local = note.column - col_offset;
    if local >= cols_per_player {
        return None;
    }
    let code = local.saturating_add(1).min(u8::MAX as usize) as u8;
    Some(code)
}

#[inline(always)]
fn is_stream_beat(beat: f32, stream_segments: &[StreamSegment]) -> bool {
    if stream_segments.is_empty() {
        return false;
    }
    let measure = (beat.floor() as i32).div_euclid(4).max(0) as usize;
    stream_segments
        .iter()
        .any(|seg| !seg.is_break && measure >= seg.start && measure < seg.end)
}

#[inline(always)]
pub fn build_scatter_points(
    notes: &[Note],
    note_time_cache_ns: &[i64],
    col_offset: usize,
    cols_per_player: usize,
    stream_segments: &[StreamSegment],
) -> Vec<ScatterPoint> {
    let mut out = Vec::with_capacity(notes.len());
    let mut foot_left = false;
    let mut row_start = 0usize;

    while row_start < notes.len() {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row {
            row_end += 1;
        }

        let row_notes = &notes[row_start..row_end];
        let row_judgment =
            judgment::aggregate_row_final_judgment(row_notes.iter().filter_map(|n| {
                if n.is_fake || !n.can_be_judged || matches!(n.note_type, NoteType::Mine) {
                    None
                } else {
                    n.result.as_ref()
                }
            }));
        let Some(judgment) = row_judgment else {
            row_start = row_end;
            continue;
        };

        let mut representative_ix: Option<usize> = None;
        let mut direction_code = 0u8;
        for (offset, n) in notes[row_start..row_end].iter().enumerate() {
            let i = row_start + offset;
            if n.is_fake || !n.can_be_judged || matches!(n.note_type, NoteType::Mine) {
                continue;
            }
            if representative_ix.is_none() && n.result.is_some() {
                representative_ix = Some(i);
            }
            if let Some(code) = local_direction_code(n, col_offset, cols_per_player) {
                direction_code = direction_code.saturating_add(code);
            }
        }

        if direction_code == 1 {
            foot_left = true;
        } else if direction_code == 4 {
            foot_left = false;
        } else if direction_code > 0 {
            foot_left = !foot_left;
        }

        let Some(idx) = representative_ix else {
            row_start = row_end;
            continue;
        };
        let t = note_time_cache_ns
            .get(idx)
            .copied()
            .map(|time_ns| (time_ns as f64 * 1.0e-9) as f32)
            .unwrap_or(0.0);
        let offset_ms = if judgment.grade == JudgeGrade::Miss {
            None
        } else {
            Some(judgment.time_error_ms)
        };

        out.push(ScatterPoint {
            time_sec: t,
            offset_ms,
            direction_code,
            is_stream: is_stream_beat(notes[idx].beat, stream_segments),
            is_left_foot: foot_left,
            miss_because_held: judgment.grade == JudgeGrade::Miss && judgment.miss_because_held,
        });

        row_start = row_end;
    }

    out
}

#[inline(always)]
fn bin_index_ms(v_ms: f32) -> i32 {
    // Mirror Simply Love behavior: floor to 1ms steps, with negative going more negative
    (v_ms / HIST_BIN_MS).floor() as i32
}

#[derive(Copy, Clone, Debug)]
struct HistMeta {
    max_abs: f32,
    worst_window_ix: usize,
    worst_observed_bin_abs: i32,
}

#[derive(Clone, Debug, Default)]
struct HistCounts {
    bins: Vec<(i32, u32)>,
    dense: Vec<u32>,
    min_bin: i32,
    max_count: u32,
}

const MAX_DENSE_HIST_SPAN: i32 = 4096;

#[inline(always)]
const fn hist_window_ix(grade: JudgeGrade) -> usize {
    match grade {
        JudgeGrade::Fantastic => 0,
        JudgeGrade::Excellent => 1,
        JudgeGrade::Great => 2,
        JudgeGrade::Decent => 3,
        JudgeGrade::WayOff => 4,
        JudgeGrade::Miss => 2,
    }
}

fn collect_hist_bins(notes: &[Note]) -> (Vec<i32>, HistMeta) {
    let mut bins = Vec::with_capacity(notes.len());
    let mut meta = HistMeta {
        max_abs: 0.0,
        worst_window_ix: hist_window_ix(JudgeGrade::Great),
        worst_observed_bin_abs: 0,
    };

    for_each_row_final_judgment(notes, |judgment| {
        if judgment.grade != JudgeGrade::Miss {
            let bin = bin_index_ms(judgment.time_error_ms);
            bins.push(bin);
            meta.max_abs = meta.max_abs.max(judgment.time_error_ms.abs());
            meta.worst_window_ix = meta.worst_window_ix.max(hist_window_ix(judgment.grade));
            meta.worst_observed_bin_abs = meta.worst_observed_bin_abs.max(bin.abs());
        }
    });

    (bins, meta)
}

fn pack_hist_counts(mut seen_bins: Vec<i32>) -> HistCounts {
    if seen_bins.is_empty() {
        return HistCounts::default();
    }

    seen_bins.sort_unstable();
    let min_bin = seen_bins[0];
    let max_bin = *seen_bins.last().unwrap_or(&min_bin);
    let span = max_bin - min_bin + 1;
    let mut counts = HistCounts {
        bins: Vec::with_capacity(seen_bins.len().min(span as usize)),
        dense: if span <= MAX_DENSE_HIST_SPAN {
            vec![0; span as usize]
        } else {
            Vec::new()
        },
        min_bin,
        max_count: 0,
    };

    let mut prev = min_bin;
    let mut run_count = 0u32;
    for bin in seen_bins {
        if !counts.dense.is_empty() {
            counts.dense[(bin - min_bin) as usize] += 1;
        }
        if bin == prev {
            run_count += 1;
            continue;
        }
        counts.max_count = counts.max_count.max(run_count);
        counts.bins.push((prev, run_count));
        prev = bin;
        run_count = 1;
    }

    counts.max_count = counts.max_count.max(run_count);
    counts.bins.push((prev, run_count));
    counts
}

#[inline(always)]
fn hist_count_at(counts: &HistCounts, bin: i32) -> u32 {
    if !counts.dense.is_empty() {
        let idx = bin - counts.min_bin;
        if idx >= 0 && (idx as usize) < counts.dense.len() {
            return counts.dense[idx as usize];
        }
        return 0;
    }

    counts
        .bins
        .binary_search_by_key(&bin, |(key, _)| *key)
        .map_or(0, |idx| counts.bins[idx].1)
}

fn smooth_hist_counts(counts: &HistCounts, worst_window_bin: i32) -> Vec<(i32, f32)> {
    let mut smoothed = Vec::with_capacity((worst_window_bin * 2 + 1).max(1) as usize);
    for bin in -worst_window_bin..=worst_window_bin {
        let mut y = 0.0_f32;
        for (offset, weight) in (-3..=3).zip(GAUSS7) {
            let sample = (bin + offset).clamp(-worst_window_bin, worst_window_bin);
            y += hist_count_at(counts, sample) as f32 * weight;
        }
        smoothed.push((bin, y));
    }
    smoothed
}

#[inline(always)]
pub fn build_histogram_ms(notes: &[Note]) -> HistogramMs {
    let (seen_bins, meta) = collect_hist_bins(notes);
    let counts = pack_hist_counts(seen_bins);
    let worst_window_ms = effective_windows_ms()[meta.worst_window_ix];
    let worst_window_bin = (worst_window_ms / HIST_BIN_MS).round() as i32;
    let smoothed = smooth_hist_counts(&counts, worst_window_bin);

    HistogramMs {
        bins: counts.bins,
        smoothed,
        max_count: counts.max_count,
        worst_observed_ms: (meta.worst_observed_bin_abs as f32) * HIST_BIN_MS,
        worst_window_ms: worst_window_ms.max(meta.max_abs),
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
    compute_window_counts_blue_ms(notes, FA_PLUS_W0_MS)
}

#[inline(always)]
pub fn compute_window_counts_10ms_blue(notes: &[Note]) -> WindowCounts {
    compute_window_counts_blue_ms(notes, FA_PLUS_W010_MS)
}

#[inline(always)]
pub fn compute_window_counts_blue_ms(notes: &[Note], blue_window_ms: f32) -> WindowCounts {
    let mut out = WindowCounts::default();
    let split_ms = if blue_window_ms.is_finite() && blue_window_ms > 0.0 {
        blue_window_ms
    } else {
        FA_PLUS_W010_MS
    };

    for_each_row_final_judgment(notes, |j| match j.grade {
        JudgeGrade::Fantastic => {
            if j.time_error_ms.abs() <= split_ms {
                out.w0 = out.w0.saturating_add(1);
            } else {
                out.w1 = out.w1.saturating_add(1);
            }
        }
        JudgeGrade::Excellent => out.w2 = out.w2.saturating_add(1),
        JudgeGrade::Great => out.w3 = out.w3.saturating_add(1),
        JudgeGrade::Decent => out.w4 = out.w4.saturating_add(1),
        JudgeGrade::WayOff => out.w5 = out.w5.saturating_add(1),
        JudgeGrade::Miss => out.miss = out.miss.saturating_add(1),
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[inline(always)]
    fn test_note(row_index: usize, column: usize, grade: JudgeGrade, time_error_ms: f32) -> Note {
        let window = match grade {
            JudgeGrade::Fantastic => Some(TimingWindow::W1),
            JudgeGrade::Excellent => Some(TimingWindow::W2),
            JudgeGrade::Great => Some(TimingWindow::W3),
            JudgeGrade::Decent => Some(TimingWindow::W4),
            JudgeGrade::WayOff => Some(TimingWindow::W5),
            JudgeGrade::Miss => None,
        };
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column,
            note_type: NoteType::Tap,
            row_index,
            result: Some(Judgment {
                time_error_ms,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(
                    time_error_ms,
                    1.0,
                ),
                grade,
                window,
                miss_because_held: false,
            }),
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[test]
    fn timing_stats_aggregate_rows() {
        let notes = vec![
            test_note(10, 0, JudgeGrade::Fantastic, -10.0),
            test_note(10, 1, JudgeGrade::Fantastic, 30.0),
            test_note(11, 0, JudgeGrade::Great, -20.0),
        ];

        let stats = compute_note_timing_stats(&notes);
        assert!((stats.mean_ms - 5.0).abs() < 0.0001);
        assert!((stats.mean_abs_ms - 25.0).abs() < 0.0001);
        assert!((stats.max_abs_ms - 30.0).abs() < 0.0001);
        assert!((stats.stddev_ms - 25.0).abs() < 0.0002);
    }

    #[test]
    fn timing_stats_use_last_tap_on_row_instead_of_worst_absolute_offset() {
        let notes = vec![
            test_note(10, 0, JudgeGrade::Decent, -45.0),
            test_note(10, 1, JudgeGrade::Great, 12.0),
            test_note(11, 0, JudgeGrade::Excellent, -6.0),
        ];

        let stats = compute_note_timing_stats(&notes);
        assert!((stats.mean_ms - 3.0).abs() < 0.0001);
        assert!((stats.mean_abs_ms - 9.0).abs() < 0.0001);
        assert!((stats.max_abs_ms - 12.0).abs() < 0.0001);
        assert!((stats.stddev_ms - 9.0).abs() < 0.0002);
    }

    #[test]
    fn timing_stats_skip_miss_rows() {
        let notes = vec![
            test_note(20, 0, JudgeGrade::Miss, 0.0),
            test_note(20, 1, JudgeGrade::Excellent, 15.0),
            test_note(21, 0, JudgeGrade::Great, -10.0),
        ];

        let stats = compute_note_timing_stats(&notes);
        assert!((stats.mean_ms + 10.0).abs() < 0.0001);
        assert!((stats.mean_abs_ms - 10.0).abs() < 0.0001);
        assert!((stats.max_abs_ms - 10.0).abs() < 0.0001);
        assert!(stats.stddev_ms.abs() < 0.0001);
    }

    #[test]
    fn histogram_ms_packs_sorted_bins_and_ignores_non_scored_notes() {
        let mut notes = vec![
            test_note(10, 0, JudgeGrade::Fantastic, -10.0),
            test_note(11, 0, JudgeGrade::Fantastic, -10.0),
            test_note(12, 0, JudgeGrade::Excellent, 30.0),
            test_note(13, 0, JudgeGrade::Excellent, 30.0),
            test_note(14, 0, JudgeGrade::Excellent, 30.0),
            test_note(15, 0, JudgeGrade::WayOff, 170.0),
        ];
        notes.push(test_note(16, 0, JudgeGrade::Miss, 0.0));

        let mut mine = test_note(17, 0, JudgeGrade::Fantastic, 12.0);
        mine.note_type = NoteType::Mine;
        notes.push(mine);

        let mut fake = test_note(18, 0, JudgeGrade::Fantastic, 18.0);
        fake.is_fake = true;
        notes.push(fake);

        let hist = build_histogram_ms(&notes);
        assert_eq!(hist.bins, vec![(-10, 2), (30, 3), (170, 1)]);
        assert_eq!(hist.max_count, 3);
        assert!((hist.worst_observed_ms - 170.0).abs() < 0.0001);
        assert!((hist.worst_window_ms - effective_windows_ms()[4]).abs() < 0.0001);
        assert_eq!(
            hist.smoothed.len(),
            ((effective_windows_ms()[4] / HIST_BIN_MS).round() as usize * 2) + 1
        );
    }

    #[test]
    fn histogram_ms_aggregates_rows_like_simply_love_offsets() {
        let notes = vec![
            test_note(10, 0, JudgeGrade::Decent, -45.0),
            test_note(10, 1, JudgeGrade::Great, 12.0),
            test_note(11, 0, JudgeGrade::Excellent, -6.0),
            test_note(12, 0, JudgeGrade::Great, 12.0),
        ];

        let hist = build_histogram_ms(&notes);
        assert_eq!(hist.bins, vec![(-6, 1), (12, 2)]);
        assert_eq!(hist.max_count, 2);
        assert!((hist.worst_observed_ms - 12.0).abs() < 0.0001);
        assert!((hist.worst_window_ms - effective_windows_ms()[2]).abs() < 0.0001);
    }

    #[test]
    fn scatter_points_use_last_tap_offset_for_rows() {
        let notes = vec![
            test_note(10, 0, JudgeGrade::Decent, -45.0),
            test_note(10, 1, JudgeGrade::Great, 12.0),
        ];
        let note_time_cache_ns = vec![1_000_000_000, 1_000_000_000];

        let scatter = build_scatter_points(&notes, &note_time_cache_ns, 0, 4, &[]);

        assert_eq!(scatter.len(), 1);
        assert_eq!(scatter[0].offset_ms, Some(12.0));
    }

    #[test]
    fn histogram_ms_empty_input_still_builds_zero_window_curve() {
        let hist = build_histogram_ms(&[]);
        let expected_window = effective_windows_ms()[hist_window_ix(JudgeGrade::Great)];
        assert!(hist.bins.is_empty());
        assert_eq!(hist.max_count, 0);
        assert_eq!(hist.worst_observed_ms, 0.0);
        assert!((hist.worst_window_ms - expected_window).abs() < 0.0001);
        assert_eq!(
            hist.smoothed.len(),
            ((expected_window / HIST_BIN_MS).round() as usize * 2) + 1
        );
        assert!(hist.smoothed.iter().all(|(_, value)| value.abs() < 0.0001));
    }

    #[test]
    fn beat_time_lookup_ns_round_trips_through_stops_and_delays() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0), (4.0, 60.0)],
                stops: vec![StopSegment {
                    beat: 4.0,
                    duration: 0.250,
                }],
                delays: vec![DelaySegment {
                    beat: 6.0,
                    duration: 0.125,
                }],
                ..TimingSegments::default()
            },
            &[],
        );

        let beat = 8.0;
        let time_ns = timing.get_time_for_beat_ns(beat);
        let time_sec = timing.get_time_for_beat(beat);

        assert!((timing_ns_to_seconds(time_ns) - time_sec).abs() < 0.000_001);
        assert!((timing.get_beat_for_time_ns(time_ns) - beat).abs() < 0.0001);
    }

    #[test]
    fn beat_info_cache_uses_ns_timing_for_freezes_and_delays() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                stops: vec![StopSegment {
                    beat: 4.0,
                    duration: 0.250,
                }],
                delays: vec![DelaySegment {
                    beat: 6.0,
                    duration: 0.125,
                }],
                ..TimingSegments::default()
            },
            &[],
        );
        let mut cache = BeatInfoCache::new(&timing);

        let stop_midpoint = timing
            .get_time_for_beat_ns(4.0)
            .saturating_add(timing_ns_from_seconds(0.100));
        let stop_info = timing.get_beat_info_from_time_ns_cached(stop_midpoint, &mut cache);
        assert!(stop_info.is_in_freeze);
        assert!(!stop_info.is_in_delay);
        assert!((stop_info.beat - 4.0).abs() < 0.0001);

        let delay_midpoint = timing
            .get_time_for_beat_ns(6.0)
            .saturating_sub(timing_ns_from_seconds(0.050));
        let delay_info = timing.get_beat_info_from_time_ns_cached(delay_midpoint, &mut cache);
        assert!(!delay_info.is_in_freeze);
        assert!(delay_info.is_in_delay);
        assert!((delay_info.beat - 6.0).abs() < 0.0001);
    }

    #[test]
    fn warp_start_row_with_stop_remains_judgable() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 100.0)],
                stops: vec![StopSegment {
                    beat: 60.0,
                    duration: 0.150,
                }],
                warps: vec![WarpSegment {
                    beat: 60.0,
                    length: 0.250,
                }],
                ..TimingSegments::default()
            },
            &[],
        );

        assert!(timing.is_judgable_at_beat(60.0));
        assert!(!timing.is_warp_at_beat(60.0));
        assert!(timing.is_warp_at_beat(60.125));
        assert!(!timing.is_judgable_at_beat(60.125));
    }

    #[test]
    fn warp_start_row_with_delay_remains_judgable() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 100.0)],
                delays: vec![DelaySegment {
                    beat: 14.0,
                    duration: 0.150,
                }],
                warps: vec![WarpSegment {
                    beat: 14.0,
                    length: 0.250,
                }],
                ..TimingSegments::default()
            },
            &[],
        );

        assert!(timing.is_judgable_at_beat(14.0));
        assert!(!timing.is_warp_at_beat(14.0));
        assert!(timing.is_warp_at_beat(14.125));
        assert!(!timing.is_judgable_at_beat(14.125));
    }

    #[test]
    fn disabled_top_windows_demote_perfect_hits_to_greats() {
        let profile = TimingProfile::default_itg_with_fa_plus();
        let disabled = [true, true, false, false, false];
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);

        let judged = classify_offset_ns_with_disabled_windows(0, &profile_ns, &disabled)
            .expect("great window should still accept perfect offsets");

        assert_eq!(judged, (JudgeGrade::Great, TimingWindow::W3));
    }

    #[test]
    fn disabled_bottom_windows_turn_outer_w4_hits_into_misses() {
        let profile = TimingProfile::default_itg_with_fa_plus();
        let disabled = [false, false, false, true, true];
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
        let offset_ns = (profile_ns.windows_ns[2] + profile_ns.windows_ns[3]) / 2;

        assert!(
            classify_offset_ns_with_disabled_windows(offset_ns, &profile_ns, &disabled).is_none()
        );
    }

    #[test]
    fn ns_largest_enabled_window_tracks_disabled_way_offs() {
        let profile = TimingProfile::default_itg_with_fa_plus();
        let disabled = [false, false, false, false, true];
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);

        let largest = largest_enabled_tap_window_ns(&profile_ns, &disabled)
            .expect("great or better stays on");

        assert_eq!(largest, profile_ns.windows_ns[3]);
    }

    #[test]
    fn ns_classifier_handles_window_edges() {
        let profile = TimingProfile::default_itg_with_fa_plus();
        let disabled = [false; 5];
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.5);
        let w0 = profile_ns
            .fa_plus_window_ns
            .expect("default profile has W0");
        let w = profile_ns.windows_ns;
        let cases = [
            (0, Some((JudgeGrade::Fantastic, TimingWindow::W0))),
            (1, Some((JudgeGrade::Fantastic, TimingWindow::W0))),
            (w0, Some((JudgeGrade::Fantastic, TimingWindow::W0))),
            (w0 + 1, Some((JudgeGrade::Fantastic, TimingWindow::W1))),
            (w[0], Some((JudgeGrade::Fantastic, TimingWindow::W1))),
            (w[0] + 1, Some((JudgeGrade::Excellent, TimingWindow::W2))),
            (w[1], Some((JudgeGrade::Excellent, TimingWindow::W2))),
            (w[1] + 1, Some((JudgeGrade::Great, TimingWindow::W3))),
            (w[2], Some((JudgeGrade::Great, TimingWindow::W3))),
            (w[2] + 1, Some((JudgeGrade::Decent, TimingWindow::W4))),
            (w[3], Some((JudgeGrade::Decent, TimingWindow::W4))),
            (w[3] + 1, Some((JudgeGrade::WayOff, TimingWindow::W5))),
            (w[4], Some((JudgeGrade::WayOff, TimingWindow::W5))),
            (w[4] + 1, None),
        ];

        for (offset_ns, expected) in cases {
            for signed_offset_ns in [offset_ns, -offset_ns] {
                assert_eq!(
                    classify_offset_ns_with_disabled_windows(
                        signed_offset_ns,
                        &profile_ns,
                        &disabled
                    ),
                    expected,
                    "offset_ns={signed_offset_ns}",
                );
            }
        }
    }
}
