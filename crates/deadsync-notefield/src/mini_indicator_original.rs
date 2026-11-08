use super::*;

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

pub(crate) const fn zmod_subtractive_counter_state(
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
            // `current_score_percent` is already floored to two decimals.  Recover
            // its integer hundredths without flooring a binary approximation such
            // as `0.29 * 100.0` to 28 instead of the Lua-equivalent 29.
            let pace = (progress.current_score_percent.clamp(0.0, 100.0) * 100.0).round();
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

pub(super) fn broken_run_end_and_next(
    segments: &[StreamSegment],
    start_index: usize,
) -> (i32, bool, usize) {
    let Some(first) = segments.get(start_index).copied() else {
        return (0, false, segments.len());
    };
    if first.is_break() {
        return (first.end() as i32, false, start_index.saturating_add(1));
    }

    let last_index = segments.len().saturating_sub(1);
    let mut end = first.end();
    let mut broken = false;
    let mut index = start_index + 1;
    while index < segments.len() {
        let segment = segments[index];
        let len = segment.end() - segment.start();
        if segment.is_break() {
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
        if !segments[index - 1].is_break() {
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

#[derive(Clone, Copy, Debug)]
struct BrokenRunSpan {
    broken_end: i32,
    segment_and_broken: u32,
}
impl BrokenRunSpan {
    const BROKEN_FLAG: u32 = 1 << 31;

    fn new(segment_index: u32, broken_end: i32, broken: bool) -> Self {
        debug_assert!(segment_index < Self::BROKEN_FLAG);
        Self {
            broken_end,
            segment_and_broken: segment_index | if broken { Self::BROKEN_FLAG } else { 0 },
        }
    }

    const fn segment_index(self) -> usize {
        (self.segment_and_broken & !Self::BROKEN_FLAG) as usize
    }

    const fn broken(self) -> bool {
        self.segment_and_broken & Self::BROKEN_FLAG != 0
    }
}

#[derive(Clone, Debug, Default)]
pub struct BrokenRunLookup {
    spans: Box<[BrokenRunSpan]>,
    broken_cursor: Cell<usize>,
    segment_cursor: Cell<usize>,
}
impl BrokenRunLookup {
    #[must_use]
    /// # Panics
    ///
    /// Panics if the function's input or state invariants are violated.
    pub fn new(segments: &[StreamSegment]) -> Self {
        assert!(
            segments.len() <= BrokenRunSpan::BROKEN_FLAG as usize,
            "broken-run lookup exceeds 31-bit segment indices"
        );
        let mut spans = Vec::with_capacity(segments.len());
        let mut index = 0;
        while let Some(segment) = segments.get(index).copied() {
            if segment.is_break() {
                spans.push(BrokenRunSpan::new(
                    index as u32,
                    segment.end() as i32,
                    false,
                ));
                index += 1;
                continue;
            }
            let (broken_end, broken, next_index) = broken_run_end_and_next(segments, index);
            spans.push(BrokenRunSpan::new(index as u32, broken_end, broken));
            index = next_index.max(index + 1);
        }
        Self {
            spans: spans.into_boxed_slice(),
            broken_cursor: Cell::new(0),
            segment_cursor: Cell::new(0),
        }
    }

    #[inline(always)]
    pub fn segment(&self, current_measure: f32) -> Option<(usize, i32, bool)> {
        if current_measure.is_nan() {
            return None;
        }
        let completed_end = if current_measure < 0.0 {
            -1
        } else {
            current_measure as i32
        };
        let index = partition_point_from_hint(&self.spans, self.broken_cursor.get(), |span| {
            debug_assert!(span.broken_end >= 0);
            span.broken_end <= completed_end
        });
        self.broken_cursor.set(index);
        self.spans
            .get(index)
            .copied()
            .map(|span| (span.segment_index(), span.broken_end, span.broken()))
    }

    /// Returns the exclusive-end counter boundary and inclusive-end timer
    /// boundary while sharing the previous source-segment hint.
    #[inline(always)]
    pub fn segment_indices(
        &self,
        segments: &[StreamSegment],
        current_measure: f32,
    ) -> (usize, usize) {
        if current_measure.is_nan() {
            return (segments.len(), segments.len());
        }
        let exclusive = partition_point_from_hint(segments, self.segment_cursor.get(), |segment| {
            current_measure >= segment.end() as f32
        });
        self.segment_cursor.set(exclusive);
        let inclusive = partition_point_from_hint(segments, exclusive, |segment| {
            current_measure > segment.end() as f32
        });
        (exclusive, inclusive)
    }
}

pub(crate) fn zmod_combo_rainbow_color(elapsed: f32, scroll: bool, combo: u32) -> [f32; 4] {
    let speed = if scroll { 0.45 } else { 0.35 };
    let offset = if scroll { combo as f32 * 0.013 } else { 0.0 };
    let hue = elapsed.mul_add(speed, offset).fract();
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
