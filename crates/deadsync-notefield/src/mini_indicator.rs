use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::stream::StreamSegment;
use deadsync_rules::timing::WindowCounts;

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

pub fn stream_segment_index_exclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if !curr_measure.is_finite() {
        return segs.len();
    }
    segs.iter()
        .position(|s| curr_measure < s.end as f32)
        .unwrap_or(segs.len())
}

pub fn stream_segment_index_inclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if !curr_measure.is_finite() {
        return segs.len();
    }
    segs.iter()
        .position(|s| curr_measure <= s.end as f32)
        .unwrap_or(segs.len())
}

pub fn zmod_broken_run_end(segs: &[StreamSegment], start_index: usize) -> (i32, bool) {
    let Some(first) = segs.get(start_index) else {
        return (0, false);
    };
    let mut end = first.end;
    let mut merged = false;
    let mut i = start_index + 1;
    while i < segs.len() {
        let seg = segs[i];
        if seg.is_break && seg.end - seg.start <= 2 && i + 1 < segs.len() && !segs[i + 1].is_break {
            merged = true;
            end = segs[i + 1].end;
            i += 2;
        } else if !seg.is_break && !first.is_break {
            end = seg.end;
            i += 1;
        } else {
            break;
        }
    }
    (end as i32, merged)
}

pub fn zmod_broken_run_segment(
    segs: &[StreamSegment],
    curr_measure: f32,
) -> Option<(usize, i32, bool)> {
    let ix = stream_segment_index_exclusive_end(segs, curr_measure);
    let seg = *segs.get(ix)?;
    if seg.is_break {
        if seg.end - seg.start <= 2 && ix > 0 && !segs[ix - 1].is_break {
            let (end, merged) = zmod_broken_run_end(segs, ix - 1);
            return Some((ix - 1, end, merged));
        }
        Some((ix, seg.end as i32, false))
    } else {
        let (end, merged) = zmod_broken_run_end(segs, ix);
        Some((ix, end, merged))
    }
}

pub fn zmod_run_timer_index(segs: &[StreamSegment], curr_measure: f32) -> Option<usize> {
    let ix = stream_segment_index_exclusive_end(segs, curr_measure);
    segs.get(ix).filter(|s| !s.is_break).map(|_| ix)
}

pub fn zmod_measure_counter_text(
    song_beat: f32,
    curr_measure: f32,
    segs: &[StreamSegment],
    index: usize,
    break_text: bool,
    lookahead: usize,
    music_rate: f32,
) -> Option<ZmodMeasureCounterText> {
    if lookahead == 0 {
        return None;
    }
    let seg = *segs.get(index)?;
    if song_beat < 0.0 {
        let target = if seg.is_break { seg.end } else { seg.start };
        return Some(ZmodMeasureCounterText::Break(
            (((target as f32 - curr_measure).ceil() + 1.0) / music_rate.max(0.001)).max(0.0) as i32,
        ));
    }
    if seg.is_break {
        let measures = if break_text {
            (seg.end - seg.start) as f32
        } else {
            (seg.end as f32 - curr_measure).ceil()
        };
        return Some(ZmodMeasureCounterText::Break(
            (measures / music_rate.max(0.001)).max(0.0) as i32,
        ));
    }
    if lookahead > 1 && index + 1 < segs.len() && segs[index + 1].is_break {
        return Some(ZmodMeasureCounterText::Ratio {
            current: (curr_measure - seg.start as f32 + 1.0).max(0.0) as i32,
            total: (seg.end - seg.start) as i32,
        });
    }
    Some(ZmodMeasureCounterText::Total((seg.end - seg.start) as i32))
}

pub fn zmod_broken_run_counter_text(
    curr_measure: f32,
    segs: &[StreamSegment],
    start_index: usize,
    end: i32,
) -> Option<ZmodMeasureCounterText> {
    let seg = *segs.get(start_index)?;
    if seg.is_break {
        return None;
    }
    if curr_measure < seg.start as f32 {
        return Some(ZmodMeasureCounterText::Break(
            (seg.start as f32 - curr_measure).ceil() as i32 + 1,
        ));
    }
    Some(ZmodMeasureCounterText::Ratio {
        current: (curr_measure - seg.start as f32 + 1.0).max(0.0) as i32,
        total: end - seg.start as i32,
    })
}

pub fn zmod_percent_from_points(points: i32, total: i32) -> f64 {
    if total <= 0 || points <= 0 {
        return 0.0;
    }
    ((points as f64 / total as f64) * 10000.0).floor() / 100.0
}

pub fn zmod_subtractive_counter_state(
    progress: &MiniIndicatorProgress,
    score_type: MiniIndicatorScoreType,
) -> (u32, bool) {
    match score_type {
        MiniIndicatorScoreType::Itg => (progress.w2, false),
        MiniIndicatorScoreType::Ex => (progress.white_count, false),
        MiniIndicatorScoreType::HardEx => (progress.white_10ms_count, true),
    }
}

pub fn zmod_subtractive_points(
    progress: &MiniIndicatorProgress,
    score_type: MiniIndicatorScoreType,
) -> i32 {
    match score_type {
        MiniIndicatorScoreType::Itg => (progress.current_possible_dp - progress.actual_dp).abs(),
        MiniIndicatorScoreType::Ex => progress.white_count as i32 * 2 + progress.w3 as i32 * 5,
        MiniIndicatorScoreType::HardEx => {
            progress.white_10ms_count as i32 * 2 + progress.w2 as i32 * 2
        }
    }
}

pub const fn zmod_mini_indicator_zoom(size: MiniIndicatorSize) -> f32 {
    match size {
        MiniIndicatorSize::Default => 0.35,
        MiniIndicatorSize::Large => 0.5,
    }
}

pub fn zmod_rival_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    if pace >= rival_pace {
        [0.0, 1.0, 1.0, 1.0]
    } else {
        [1.0, 0.0, 0.0, 1.0]
    }
}

pub fn zmod_pacemaker_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    if pace >= rival_pace {
        [0.99, 0.51, 1.0, 1.0]
    } else {
        [1.0, 0.0, 0.0, 1.0]
    }
}

pub(crate) fn rgba8(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
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
            let (count, _) = zmod_subtractive_counter_state(progress, params.score_type);
            if params.subtractive_display == MiniIndicatorSubtractiveDisplay::Points
                || count == 0
                || progress.w3 > 0
                || progress.w4 > 0
                || progress.w5 > 0
                || progress.miss > 0
            {
                Some(ZmodMiniIndicatorOutput {
                    text: ZmodMiniIndicatorText::SignedPercent {
                        value: progress.lost_percent,
                        negative: true,
                    },
                    color: zmod_indicator_default_color(progress.kept_percent),
                })
            } else {
                Some(ZmodMiniIndicatorOutput {
                    text: ZmodMiniIndicatorText::NegativeInt(count),
                    color: rgba8(0xff, 0x55, 0xcc),
                })
            }
        }
        MiniIndicatorMode::PredictiveScoring => Some(ZmodMiniIndicatorOutput {
            text: ZmodMiniIndicatorText::Percent(progress.kept_percent),
            color: zmod_indicator_default_color(progress.kept_percent),
        }),
        MiniIndicatorMode::PaceScoring => Some(ZmodMiniIndicatorOutput {
            text: ZmodMiniIndicatorText::SignedPercent {
                value: progress.pace_percent.abs(),
                negative: progress.pace_percent < 0.0,
            },
            color: zmod_indicator_default_color(progress.current_score_percent),
        }),
        MiniIndicatorMode::RivalScoring => {
            let target = params.rival_score_percent * progress.current_possible_ratio;
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: (progress.current_score_percent - target).abs(),
                    negative: progress.current_score_percent < target,
                },
                color: zmod_rival_color(progress.current_score_percent, target),
            })
        }
        MiniIndicatorMode::Pacemaker => {
            let target = params.target_score_percent * progress.current_possible_ratio;
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: (progress.current_score_percent - target).abs(),
                    negative: progress.current_score_percent < target,
                },
                color: zmod_pacemaker_color(progress.current_score_percent * 100.0, target * 100.0),
            })
        }
        MiniIndicatorMode::StreamProg => {
            params.stream_completion.map(|c| ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(c * 100.0),
                color: [0.0, 1.0, 0.5, 1.0],
            })
        }
        MiniIndicatorMode::None => None,
    }
}

pub fn zmod_combo_glow_color(color1: [f32; 4], color2: [f32; 4], elapsed: f32) -> [f32; 4] {
    let t = ((elapsed * std::f32::consts::TAU * 1.25).sin() + 1.0) * 0.5;
    [
        color1[0] + (color2[0] - color1[0]) * t,
        color1[1] + (color2[1] - color1[1]) * t,
        color1[2] + (color2[2] - color1[2]) * t,
        color1[3] + (color2[3] - color1[3]) * t,
    ]
}

pub fn zmod_combo_glow_pair(grade: JudgeGrade, quint: bool) -> ([f32; 4], [f32; 4]) {
    if quint {
        return (rgba8(247, 192, 254), rgba8(233, 40, 255));
    }
    match grade {
        JudgeGrade::Fantastic => (rgba8(200, 255, 255), rgba8(107, 240, 255)),
        JudgeGrade::Excellent => (rgba8(255, 220, 100), rgba8(226, 156, 24)),
        JudgeGrade::Great => (rgba8(170, 255, 120), rgba8(102, 201, 85)),
        JudgeGrade::Decent => (rgba8(210, 150, 255), rgba8(180, 92, 255)),
        _ => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
    }
}

pub fn zmod_combo_solid_color(grade: JudgeGrade, quint: bool) -> [f32; 4] {
    if quint && grade == JudgeGrade::Fantastic {
        return rgba8(233, 40, 255);
    }
    match grade {
        JudgeGrade::Fantastic => rgba8(33, 204, 232),
        JudgeGrade::Excellent => rgba8(226, 156, 24),
        JudgeGrade::Great => rgba8(102, 201, 85),
        JudgeGrade::Decent => rgba8(180, 92, 255),
        JudgeGrade::WayOff => rgba8(201, 133, 94),
        JudgeGrade::Miss => [1.0, 1.0, 1.0, 1.0],
    }
}

pub fn zmod_indicator_default_color(score_percent: f64) -> [f32; 4] {
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

pub fn zmod_indicator_detailed_color(score_percent: f64) -> [f32; 4] {
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

pub fn zmod_combo_rainbow_color(elapsed: f32, scroll: bool, combo: u32) -> [f32; 4] {
    let g =
        (elapsed.fract() * 5.571 + if scroll { combo as f32 * 0.078 } else { 0.0 }).clamp(0.0, 1.0);
    [1.0, g, 0.0, 1.0]
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
