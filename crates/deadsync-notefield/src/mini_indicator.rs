use deadsync_gameplay::partition_point_from_hint;
use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::stream::StreamSegment;
use deadsync_rules::timing::WindowCounts;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorScoreType {
    Itg,
    Ex,
    HardEx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorSize {
    Default,
    Large,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorMode {
    None,
    SubtractiveScoring,
    PredictiveScoring,
    PaceScoring,
    RivalScoring,
    Pacemaker,
    StreamProg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorColorStyle {
    Default,
    Detailed,
    Combo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorSubtractiveDisplay {
    CountThenPercent,
    Points,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MiniIndicatorProgress {
    pub kept_percent: f64,
    pub lost_percent: f64,
    pub pace_percent: f64,
    pub current_score_percent: f64,
    pub current_possible_ratio: f64,
    pub current_possible_dp: i32,
    pub actual_dp: i32,
    pub white_count: u32,
    pub white_10ms_count: u32,
    pub w2: u32,
    pub w3: u32,
    pub w4: u32,
    pub w5: u32,
    pub miss: u32,
    pub let_go: u32,
    pub mines_hit: u32,
    pub judged_any: bool,
}

/// Matches zmod's missed-target test: the score already earned plus every
/// remaining point cannot reach the configured target.
#[inline(always)]
pub fn zmod_target_score_missed(
    progress: &MiniIndicatorProgress,
    target_score_percent: f64,
) -> bool {
    if !progress.judged_any {
        return false;
    }
    let attainable = progress.current_score_percent.clamp(0.0, 100.0)
        + (1.0 - progress.current_possible_ratio.clamp(0.0, 1.0)) * 100.0;
    attainable < target_score_percent.clamp(0.0, 100.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZmodMeasureCounterText {
    Ratio { current: i32, total: i32 },
    Break(i32),
    Total(i32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZmodMiniIndicatorText {
    NegativeInt(u32),
    SignedPercent { value: f64, negative: bool },
    Percent(f64),
}

#[derive(Clone, Copy, Debug)]
pub struct ZmodMiniIndicatorParams {
    pub mode: MiniIndicatorMode,
    pub color_style: MiniIndicatorColorStyle,
    pub subtractive_display: MiniIndicatorSubtractiveDisplay,
    pub score_type: MiniIndicatorScoreType,
    pub combo_color: [f32; 4],
    pub is_failing: bool,
    pub life: f32,
    pub rival_score_percent: f64,
    pub target_score_percent: f64,
    pub stream_completion: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZmodMiniIndicatorOutput {
    pub text: ZmodMiniIndicatorText,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZmodComboColorStyle {
    None,
    Rainbow,
    RainbowScroll,
    Glow,
    Solid,
}

#[derive(Clone, Copy, Debug)]
pub struct ZmodComboColorParams {
    pub style: ZmodComboColorStyle,
    pub full_combo_mode: bool,
    pub combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub quint_active: bool,
    pub elapsed_s: f32,
}

pub const fn zmod_combo_quint_active(show_fa_plus_window: bool, counts: WindowCounts) -> bool {
    show_fa_plus_window
        && counts.w0 > 0
        && counts.w1 == 0
        && counts.w2 == 0
        && counts.w3 == 0
        && counts.w4 == 0
        && counts.w5 == 0
        && counts.miss == 0
}

pub const fn zmod_resolved_mini_indicator_mode(
    mode: MiniIndicatorMode,
    subtractive: bool,
    pacemaker: bool,
) -> MiniIndicatorMode {
    match mode {
        MiniIndicatorMode::None if subtractive => MiniIndicatorMode::SubtractiveScoring,
        MiniIndicatorMode::None if pacemaker => MiniIndicatorMode::Pacemaker,
        _ => mode,
    }
}

pub(crate) fn stream_segment_index_exclusive_end(
    segs: &[StreamSegment],
    curr_measure: f32,
) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure >= s.end as f32)
}

pub(crate) fn stream_segment_index_inclusive_end(
    segs: &[StreamSegment],
    curr_measure: f32,
) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure > s.end as f32)
}

#[derive(Clone, Copy, Debug)]
struct BrokenRunSpan {
    end: f32,
    segment_index: usize,
    broken_end: i32,
    broken: bool,
}

/// Song-lifetime prefix totals for the StreamProg mini indicator.
///
/// The gameplay screen owns one lookup per player for the song lifetime. It is
/// built while entering gameplay, uses one boxed allocation sized to the chart,
/// and is accessed only on the game/render thread. A `Cell` retains the previous
/// partition boundary, so forward frames usually inspect one nearby entry while
/// seeks remain logarithmic through an exponential bracket and binary search.
/// There are no misses, eviction, synchronization, gameplay-time allocation, or
/// song-time destruction. `storage_bytes` exposes retained entry storage; the
/// worst frame is O(log n), and the boxed entries are freed on screen exit.
#[derive(Clone, Debug, Default)]
pub struct StreamProgressLookup {
    segments: Box<[StreamProgressEntry]>,
    cursor: Cell<usize>,
}

#[derive(Clone, Copy, Debug)]
struct StreamProgressEntry {
    start: f32,
    end: f32,
    stream_before: f64,
    is_break: bool,
}

impl StreamProgressLookup {
    pub fn new(segments: &[StreamSegment]) -> Self {
        let mut stream_before = 0.0;
        let entries = segments
            .iter()
            .map(|segment| {
                let start = segment.start as f32;
                let end = segment.end as f32;
                let entry = StreamProgressEntry {
                    start,
                    end,
                    stream_before,
                    is_break: segment.is_break,
                };
                if !segment.is_break {
                    stream_before += f64::from(end - start);
                }
                entry
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            segments: entries,
            cursor: Cell::new(0),
        }
    }

    pub fn storage_bytes(&self) -> usize {
        self.segments
            .len()
            .saturating_mul(std::mem::size_of::<StreamProgressEntry>())
    }

    #[inline(always)]
    pub fn completion_for_beat(&self, total_stream_measures: f64, beat_floor: f32) -> Option<f64> {
        if total_stream_measures <= 0.0 || self.segments.is_empty() {
            return None;
        }
        let current = if beat_floor.is_finite() {
            (beat_floor / 4.0).ceil().max(0.0)
        } else {
            0.0
        };
        let index = partition_point_from_hint(&self.segments, self.cursor.get(), |segment| {
            current >= segment.end
        });
        self.cursor.set(index);
        let Some(segment) = self.segments.get(index) else {
            let completed = self.segments.last().map_or(0.0, |last| {
                last.stream_before
                    + if last.is_break {
                        0.0
                    } else {
                        f64::from(last.end - last.start)
                    }
            });
            return Some((completed / total_stream_measures).clamp(0.0, 1.0));
        };
        let partial = if segment.is_break || current <= segment.start {
            0.0
        } else {
            f64::from(current.min(segment.end) - segment.start)
        };
        Some(((segment.stream_before + partial) / total_stream_measures).clamp(0.0, 1.0))
    }
}

/// Song-lifetime canonical spans for the broken-run measure counter.
///
/// The gameplay screen owns one lookup per player for the song lifetime.
/// Construction is one linear pass at screen entry and stores canonical spans
/// in a byte-bounded boxed slice. A game/render-thread-only `Cell` retains the
/// previous boundary, making forward frames O(1) in the usual case; exponential
/// bracketing keeps seeks O(log n). There are no misses, eviction,
/// synchronization, or gameplay-time allocations. `storage_bytes` exposes the
/// retained span storage, and all storage is released on screen exit.
#[derive(Clone, Debug, Default)]
pub struct BrokenRunLookup {
    spans: Box<[BrokenRunSpan]>,
    cursor: Cell<usize>,
}

impl BrokenRunLookup {
    pub fn new(segments: &[StreamSegment]) -> Self {
        let mut spans = Vec::with_capacity(segments.len());
        let mut index = 0;
        while let Some(segment) = segments.get(index).copied() {
            if segment.is_break {
                spans.push(BrokenRunSpan {
                    end: segment.end as f32,
                    segment_index: index,
                    broken_end: segment.end as i32,
                    broken: false,
                });
                index += 1;
                continue;
            }
            let (broken_end, broken, next_index) = broken_run_end_and_next(segments, index);
            spans.push(BrokenRunSpan {
                end: broken_end as f32,
                segment_index: index,
                broken_end,
                broken,
            });
            index = next_index.max(index + 1);
        }
        Self {
            spans: spans.into_boxed_slice(),
            cursor: Cell::new(0),
        }
    }

    pub fn storage_bytes(&self) -> usize {
        self.spans
            .len()
            .saturating_mul(std::mem::size_of::<BrokenRunSpan>())
    }

    #[inline(always)]
    pub fn segment(&self, current_measure: f32) -> Option<(usize, i32, bool)> {
        if current_measure.is_nan() {
            return None;
        }
        let index = partition_point_from_hint(&self.spans, self.cursor.get(), |span| {
            current_measure >= span.end
        });
        self.cursor.set(index);
        self.spans
            .get(index)
            .map(|span| (span.segment_index, span.broken_end, span.broken))
    }
}

fn broken_run_end_and_next(segments: &[StreamSegment], start_index: usize) -> (i32, bool, usize) {
    let Some(first) = segments.get(start_index).copied() else {
        return (0, false, segments.len());
    };
    if first.is_break {
        return (first.end as i32, false, start_index.saturating_add(1));
    }

    let last_index = segments.len().saturating_sub(1);
    let mut end = first.end;
    let mut broken = false;
    let mut index = start_index + 1;
    while index < segments.len() {
        let segment = segments[index];
        let len = segment.end - segment.start;
        if segment.is_break {
            if len < 4 && index != last_index {
                end += len;
                broken = true;
                index += 1;
                continue;
            }
            break;
        }
        broken = true;
        end += len;
        if !segments[index - 1].is_break {
            end += 1;
        }
        index += 1;
    }
    (end as i32, broken, index)
}

pub fn zmod_broken_run_end(segs: &[StreamSegment], start_index: usize) -> (i32, bool) {
    let (end, broken, _) = broken_run_end_and_next(segs, start_index);
    (end, broken)
}

#[cfg(test)]
pub(crate) fn zmod_broken_run_segment(
    segs: &[StreamSegment],
    curr_measure: f32,
) -> Option<(usize, i32, bool)> {
    for (i, seg) in segs.iter().copied().enumerate() {
        if seg.is_break {
            if curr_measure < seg.end as f32 {
                return Some((i, seg.end as i32, false));
            }
            continue;
        }
        let (end, broken) = zmod_broken_run_end(segs, i);
        if curr_measure < end as f32 {
            return Some((i, end, broken));
        }
    }
    None
}

pub(crate) fn zmod_run_timer_index(segs: &[StreamSegment], curr_measure: f32) -> Option<usize> {
    let index = stream_segment_index_inclusive_end(segs, curr_measure);
    if index < segs.len() {
        Some(index)
    } else {
        None
    }
}

pub(crate) fn zmod_measure_counter_text(
    curr_beat_floor: f32,
    curr_measure: f32,
    segs: &[StreamSegment],
    index: usize,
    is_lookahead: bool,
    lookahead: usize,
    multiplier: f32,
) -> Option<ZmodMeasureCounterText> {
    if segs.is_empty() {
        return None;
    }

    let mut stream_index = index as isize;
    let beat_div4 = curr_beat_floor / 4.0;

    if curr_measure < 0.0 {
        if !is_lookahead {
            let first = segs[0];
            if !first.is_break {
                let value = ((-beat_div4) + (1.0 * multiplier)).floor() as i32;
                return Some(ZmodMeasureCounterText::Break(value));
            }
            let len = (first.end - first.start) as i32;
            let value_unscaled = (-beat_div4).floor() as i32 + 1 + len;
            let value = ((value_unscaled as f32) * multiplier).floor() as i32;
            return Some(ZmodMeasureCounterText::Break(value));
        }
        if !segs[0].is_break {
            stream_index -= 1;
        }
    }

    let seg = stream_index
        .try_into()
        .ok()
        .and_then(|i: usize| segs.get(i).copied())?;

    let segment_start = seg.start as f32;
    let segment_end = seg.end as f32;
    let seg_len = ((segment_end - segment_start) * multiplier).floor() as i32;
    let curr_count = (((beat_div4 - segment_start) * multiplier).floor() as i32) + 1;

    if seg.is_break {
        if lookahead == 0 {
            return None;
        }
        if is_lookahead {
            Some(ZmodMeasureCounterText::Break(seg_len))
        } else {
            Some(ZmodMeasureCounterText::Break(seg_len - curr_count + 1))
        }
    } else if !is_lookahead && curr_count != 0 {
        Some(ZmodMeasureCounterText::Ratio {
            current: curr_count,
            total: seg_len,
        })
    } else {
        Some(ZmodMeasureCounterText::Total(seg_len))
    }
}

pub(crate) fn zmod_broken_run_counter_text(
    curr_measure: f32,
    segs: &[StreamSegment],
    start_index: usize,
    end: i32,
) -> Option<ZmodMeasureCounterText> {
    let seg = *segs.get(start_index)?;
    if seg.is_break {
        return None;
    }
    if curr_measure < 0.0 {
        let first = segs[0];
        if first.is_break {
            let first_len = (first.end - first.start) as i32;
            let value = (-curr_measure).floor() as i32 + 1 + first_len;
            return Some(ZmodMeasureCounterText::Break(value));
        }
        let value = (-curr_measure).floor() as i32 + 1;
        return Some(ZmodMeasureCounterText::Break(value));
    }

    let curr_count = (curr_measure - seg.start as f32).floor() as i32 + 1;
    let len = end - seg.start as i32;
    if curr_count != 0 {
        Some(ZmodMeasureCounterText::Ratio {
            current: curr_count,
            total: len,
        })
    } else {
        Some(ZmodMeasureCounterText::Total(len))
    }
}

pub fn zmod_percent_from_points(points: i32, total: i32) -> f64 {
    if total <= 0 || points <= 0 {
        return 0.0;
    }
    ((points as f64 / total as f64) * 10000.0).floor() / 100.0
}

pub(crate) fn zmod_subtractive_counter_state(
    progress: &MiniIndicatorProgress,
    score_type: MiniIndicatorScoreType,
) -> (u32, bool) {
    let forced_percent = progress.w3 > 0
        || progress.w4 > 0
        || progress.w5 > 0
        || progress.miss > 0
        || progress.let_go > 0
        || progress.mines_hit > 0;
    match score_type {
        MiniIndicatorScoreType::Itg => (progress.w2, forced_percent || progress.w2 > 10),
        MiniIndicatorScoreType::Ex => (
            progress.white_count,
            forced_percent || progress.w2 > 0 || progress.white_count > 10,
        ),
        MiniIndicatorScoreType::HardEx => (
            progress.white_10ms_count,
            forced_percent || progress.w2 > 0 || progress.white_10ms_count > 10,
        ),
    }
}

pub(crate) fn zmod_subtractive_points(
    progress: &MiniIndicatorProgress,
    score_type: MiniIndicatorScoreType,
) -> u32 {
    match score_type {
        MiniIndicatorScoreType::Itg => progress
            .current_possible_dp
            .saturating_sub(progress.actual_dp)
            .max(0) as u32,
        MiniIndicatorScoreType::Ex => progress
            .white_count
            .saturating_add(progress.w2.saturating_mul(3))
            .saturating_add(progress.w3.saturating_mul(5))
            .saturating_add(
                progress
                    .w4
                    .saturating_add(progress.w5)
                    .saturating_add(progress.miss)
                    .saturating_mul(7),
            )
            .saturating_add(progress.let_go.saturating_mul(2))
            .saturating_add(progress.mines_hit.saturating_mul(2)),
        MiniIndicatorScoreType::HardEx => progress
            .white_10ms_count
            .saturating_add(progress.w2.saturating_mul(5))
            .saturating_add(
                progress
                    .w3
                    .saturating_add(progress.w4)
                    .saturating_add(progress.w5)
                    .saturating_add(progress.miss)
                    .saturating_mul(7),
            )
            .saturating_add(progress.let_go.saturating_mul(2))
            .saturating_add(progress.mines_hit.saturating_mul(2)),
    }
}

pub const fn zmod_mini_indicator_zoom(size: MiniIndicatorSize) -> f32 {
    match size {
        MiniIndicatorSize::Default => 0.35,
        MiniIndicatorSize::Large => 0.5,
    }
}

pub(crate) fn zmod_rival_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace)).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

pub(crate) fn zmod_pacemaker_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace) / 100.0).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

pub(crate) fn rgba8(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

fn zmod_indicator_score_color(score_percent: f64, style: MiniIndicatorColorStyle) -> [f32; 4] {
    match style {
        MiniIndicatorColorStyle::Default => zmod_indicator_default_color(score_percent),
        MiniIndicatorColorStyle::Detailed => zmod_indicator_detailed_color(score_percent),
        MiniIndicatorColorStyle::Combo => zmod_indicator_default_color(score_percent),
    }
}

fn zmod_mini_indicator_score_color(
    score_percent: f64,
    params: ZmodMiniIndicatorParams,
) -> [f32; 4] {
    match params.color_style {
        MiniIndicatorColorStyle::Combo => params.combo_color,
        style => zmod_indicator_score_color(score_percent, style),
    }
}

pub(crate) fn zmod_stream_prog_color(completion: f64) -> [f32; 4] {
    if completion >= 0.9 {
        [
            0.0,
            1.0,
            ((completion - 0.9) * 10.0).clamp(0.0, 1.0) as f32,
            1.0,
        ]
    } else if completion >= 0.5 {
        [
            ((0.9 - completion) * 10.0 / 4.0).clamp(0.0, 1.0) as f32,
            1.0,
            0.0,
            1.0,
        ]
    } else {
        [
            1.0,
            ((completion - 0.2) * 10.0 / 3.0).clamp(0.0, 1.0) as f32,
            0.0,
            1.0,
        ]
    }
}

pub fn zmod_mini_indicator_output(
    progress: &MiniIndicatorProgress,
    params: ZmodMiniIndicatorParams,
) -> Option<ZmodMiniIndicatorOutput> {
    if params.mode == MiniIndicatorMode::None || !progress.judged_any {
        return None;
    }
    match params.mode {
        MiniIndicatorMode::SubtractiveScoring => {
            if params.subtractive_display == MiniIndicatorSubtractiveDisplay::Points {
                let points = zmod_subtractive_points(progress, params.score_type);
                let score = progress.kept_percent.clamp(0.0, 100.0);
                return Some(ZmodMiniIndicatorOutput {
                    text: ZmodMiniIndicatorText::NegativeInt(points),
                    color: zmod_mini_indicator_score_color(score, params),
                });
            }

            let (count, entered_percent_mode) =
                zmod_subtractive_counter_state(progress, params.score_type);
            if !(entered_percent_mode || params.is_failing || params.life <= 0.0) && count > 0 {
                Some(ZmodMiniIndicatorOutput {
                    text: ZmodMiniIndicatorText::NegativeInt(count),
                    color: if params.color_style == MiniIndicatorColorStyle::Combo {
                        params.combo_color
                    } else {
                        rgba8(0xff, 0x55, 0xcc)
                    },
                })
            } else {
                let score = progress.kept_percent.clamp(0.0, 100.0);
                Some(ZmodMiniIndicatorOutput {
                    text: ZmodMiniIndicatorText::SignedPercent {
                        value: progress.lost_percent.clamp(0.0, 100.0),
                        negative: true,
                    },
                    color: zmod_mini_indicator_score_color(score, params),
                })
            }
        }
        MiniIndicatorMode::PredictiveScoring => {
            let score = progress.kept_percent.clamp(0.0, 100.0);
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(score),
                color: zmod_mini_indicator_score_color(score, params),
            })
        }
        MiniIndicatorMode::PaceScoring => {
            let pace = progress.pace_percent.clamp(0.0, 100.0);
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(pace),
                color: zmod_mini_indicator_score_color(pace, params),
            })
        }
        MiniIndicatorMode::RivalScoring => {
            let pace = progress.current_score_percent.clamp(0.0, 100.0);
            let rival_score = params.rival_score_percent.clamp(0.0, 100.0);
            let target =
                (progress.current_possible_ratio * 10000.0 * rival_score).floor() / 10000.0;
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: (pace - target).abs(),
                    negative: pace < target,
                },
                color: if params.color_style == MiniIndicatorColorStyle::Combo {
                    params.combo_color
                } else {
                    zmod_rival_color(pace, target)
                },
            })
        }
        MiniIndicatorMode::Pacemaker => {
            let pace = (progress.current_score_percent.clamp(0.0, 100.0) * 100.0).floor();
            let target_ratio = (params.target_score_percent / 100.0).clamp(0.0, 1.0);
            let target =
                (progress.current_possible_ratio * 1_000_000.0 * target_ratio).floor() / 100.0;
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: ((pace - target).abs().floor() / 100.0).max(0.0),
                    negative: pace < target,
                },
                color: if params.color_style == MiniIndicatorColorStyle::Combo {
                    params.combo_color
                } else {
                    zmod_pacemaker_color(pace, target)
                },
            })
        }
        MiniIndicatorMode::StreamProg => {
            params.stream_completion.map(|c| ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent((c * 100.0).clamp(0.0, 100.0)),
                color: zmod_stream_prog_color(c),
            })
        }
        MiniIndicatorMode::None => None,
    }
}

pub(crate) fn zmod_combo_glow_color(color1: [f32; 4], color2: [f32; 4], elapsed: f32) -> [f32; 4] {
    let effect_period = 0.8_f32;
    let through = (elapsed / effect_period).fract();
    let t = ((through * std::f32::consts::TAU).sin() + 1.0) * 0.5;
    [
        color1[0] + (color2[0] - color1[0]) * t,
        color1[1] + (color2[1] - color1[1]) * t,
        color1[2] + (color2[2] - color1[2]) * t,
        1.0,
    ]
}

pub(crate) fn zmod_combo_glow_pair(grade: JudgeGrade, quint: bool) -> ([f32; 4], [f32; 4]) {
    if quint && grade == JudgeGrade::Fantastic {
        return (rgba8(247, 192, 254), rgba8(233, 40, 255));
    }
    match grade {
        JudgeGrade::Fantastic => (rgba8(200, 255, 255), rgba8(107, 240, 255)),
        JudgeGrade::Excellent => (rgba8(253, 255, 201), rgba8(253, 219, 133)),
        JudgeGrade::Great => (rgba8(201, 255, 201), rgba8(148, 254, 193)),
        _ => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
    }
}

pub(crate) fn zmod_combo_solid_color(grade: JudgeGrade, quint: bool) -> [f32; 4] {
    if quint && grade == JudgeGrade::Fantastic {
        return rgba8(233, 40, 255);
    }
    match grade {
        JudgeGrade::Fantastic => rgba8(33, 204, 232),
        JudgeGrade::Excellent => rgba8(226, 156, 24),
        JudgeGrade::Great => rgba8(102, 201, 85),
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

pub(crate) fn zmod_indicator_default_color(score_percent: f64) -> [f32; 4] {
    if score_percent >= 96.0 {
        rgba8(33, 204, 232)
    } else if score_percent >= 89.0 {
        rgba8(226, 156, 24)
    } else if score_percent >= 80.0 {
        rgba8(102, 201, 85)
    } else if score_percent >= 68.0 {
        rgba8(180, 92, 255)
    } else {
        rgba8(255, 48, 48)
    }
}

pub(crate) fn zmod_indicator_detailed_color(score_percent: f64) -> [f32; 4] {
    if score_percent >= 99.0 {
        [1.0, 0.0, 1.0, 1.0]
    } else if score_percent >= 98.0 {
        rgba8(37, 110, 206)
    } else if score_percent >= 96.0 {
        [1.0, 1.0, 1.0, 1.0]
    } else if score_percent >= 94.0 {
        rgba8(253, 163, 7)
    } else if score_percent >= 90.0 {
        rgba8(121, 169, 1)
    } else if score_percent >= 85.0 {
        rgba8(185, 50, 226)
    } else {
        [1.0, 0.0, 0.0, 1.0]
    }
}

pub(crate) fn zmod_combo_rainbow_color(elapsed: f32, scroll: bool, combo: u32) -> [f32; 4] {
    let speed = if scroll { 0.45 } else { 0.35 };
    let offset = if scroll { combo as f32 * 0.013 } else { 0.0 };
    let hue = (elapsed * speed + offset).fract();
    let h6 = hue * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let q = 1.0 - f;
    match i.rem_euclid(6) {
        0 => [1.0, f, 0.0, 1.0],
        1 => [q, 1.0, 0.0, 1.0],
        2 => [0.0, 1.0, f, 1.0],
        3 => [0.0, q, 1.0, 1.0],
        4 => [f, 0.0, 1.0, 1.0],
        _ => [1.0, 0.0, q, 1.0],
    }
}

fn zmod_combo_grade(params: ZmodComboColorParams) -> Option<JudgeGrade> {
    if params.full_combo_mode {
        params.full_combo_grade
    } else {
        params.current_combo_grade
    }
}

fn zmod_full_combo_rainbow_active(grade: Option<JudgeGrade>) -> bool {
    matches!(
        grade,
        Some(JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great)
    )
}

pub fn zmod_static_combo_color(params: ZmodComboColorParams) -> [f32; 4] {
    let grade = zmod_combo_grade(params).unwrap_or(JudgeGrade::Miss);
    zmod_combo_solid_color(grade, params.quint_active)
}

pub fn zmod_resolved_combo_color(params: ZmodComboColorParams) -> [f32; 4] {
    match params.style {
        ZmodComboColorStyle::None => [1.0, 1.0, 1.0, 1.0],
        ZmodComboColorStyle::Rainbow => {
            let grade = zmod_combo_grade(params);
            if params.full_combo_mode && !zmod_full_combo_rainbow_active(grade) {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                zmod_combo_rainbow_color(params.elapsed_s, false, params.combo)
            }
        }
        ZmodComboColorStyle::RainbowScroll => {
            zmod_combo_rainbow_color(params.elapsed_s, true, params.combo)
        }
        ZmodComboColorStyle::Glow => {
            let grade = zmod_combo_grade(params).unwrap_or(JudgeGrade::Miss);
            let (a, b) = zmod_combo_glow_pair(grade, params.quint_active);
            zmod_combo_glow_color(a, b, params.elapsed_s)
        }
        ZmodComboColorStyle::Solid => zmod_static_combo_color(params),
    }
}

pub fn zmod_stream_prog_completion_for_beat(
    total_stream_measures: f64,
    segs: &[StreamSegment],
    beat_floor: f32,
) -> Option<f64> {
    if total_stream_measures <= 0.0 || segs.is_empty() {
        return None;
    }
    let curr = if beat_floor.is_finite() {
        (beat_floor / 4.0).ceil().max(0.0)
    } else {
        0.0
    };
    let mut done = 0.0;
    for seg in segs {
        if seg.is_break {
            continue;
        }
        let start = seg.start as f32;
        let end = seg.end as f32;
        if curr >= end {
            done += (end - start) as f64;
        } else if curr > start {
            done += (curr - start) as f64;
        }
    }
    Some((done / total_stream_measures).clamp(0.0, 1.0))
}

#[cfg(test)]
mod lookup_tests {
    use super::*;

    fn segments() -> Vec<StreamSegment> {
        vec![
            StreamSegment {
                start: 0,
                end: 4,
                is_break: false,
            },
            StreamSegment {
                start: 4,
                end: 6,
                is_break: true,
            },
            StreamSegment {
                start: 6,
                end: 10,
                is_break: false,
            },
            StreamSegment {
                start: 10,
                end: 15,
                is_break: true,
            },
            StreamSegment {
                start: 15,
                end: 20,
                is_break: false,
            },
        ]
    }

    #[test]
    fn stream_progress_lookup_matches_full_scan_at_boundaries_and_invalid_beats() {
        let segments = segments();
        let lookup = StreamProgressLookup::new(&segments);
        let total = 13.0;
        for beat in [
            f32::NEG_INFINITY,
            -4.0,
            0.0,
            12.0,
            16.0,
            20.0,
            24.0,
            39.9,
            40.0,
            60.0,
            80.0,
            f32::INFINITY,
            f32::NAN,
        ] {
            assert_eq!(
                lookup.completion_for_beat(total, beat),
                zmod_stream_prog_completion_for_beat(total, &segments, beat),
                "beat={beat:?}",
            );
        }
        assert_eq!(lookup.completion_for_beat(0.0, 12.0), None);
        assert_eq!(
            StreamProgressLookup::default().completion_for_beat(1.0, 0.0),
            None
        );
    }

    #[test]
    fn broken_run_lookup_matches_legacy_scan_across_spans() {
        let segments = segments();
        let lookup = BrokenRunLookup::new(&segments);
        for quarter_measure in -4..=88 {
            let current = quarter_measure as f32 * 0.25;
            assert_eq!(
                lookup.segment(current),
                zmod_broken_run_segment(&segments, current),
                "measure={current}",
            );
        }
        assert_eq!(lookup.segment(f32::NAN), None);
        assert_eq!(BrokenRunLookup::default().segment(0.0), None);
    }

    #[test]
    fn cursor_lookups_match_legacy_across_forward_and_backward_seeks() {
        let segments = segments();
        let stream = StreamProgressLookup::new(&segments);
        let broken = BrokenRunLookup::new(&segments);
        let total = 13.0;
        for measure in [0.0, 18.0, 2.0, 9.75, -1.0, 20.0, 5.5, 14.0, 0.25] {
            let beat = measure * 4.0;
            assert_eq!(
                stream.completion_for_beat(total, beat),
                zmod_stream_prog_completion_for_beat(total, &segments, beat),
                "measure={measure}",
            );
            assert_eq!(
                broken.segment(measure),
                zmod_broken_run_segment(&segments, measure),
                "measure={measure}",
            );
        }
    }
}
