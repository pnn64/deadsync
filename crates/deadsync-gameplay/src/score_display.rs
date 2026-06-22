#[derive(Clone, Copy, Debug, Default)]
pub struct ExScoreInputs {
    pub counts: WindowCounts,
    pub counts_10ms: WindowCounts,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

pub fn ex_score_inputs_from_display(
    counts: WindowCounts,
    counts_10ms: WindowCounts,
    stage: ItgScoreStage,
) -> ExScoreInputs {
    ExScoreInputs {
        counts,
        counts_10ms,
        holds_held_for_score: stage.holds_held_for_score,
        holds_let_go_for_score: stage.holds_let_go_for_score,
        rolls_held_for_score: stage.rolls_held_for_score,
        rolls_let_go_for_score: stage.rolls_let_go_for_score,
        mines_hit_for_score: stage.mines_hit_for_score,
    }
}

pub fn ex_score_data_from_display_inputs(
    inputs: ExScoreInputs,
    carry: CourseDisplayCarry,
    totals: CourseDisplayTotals,
) -> judgment::ExScoreData {
    let (holds_held, holds_resolved) = judgment::scored_hold_totals_with_carry(
        inputs.holds_held_for_score,
        inputs.holds_let_go_for_score,
        carry.holds_held_for_score,
        carry.holds_let_go_for_score,
    );
    let (rolls_held, rolls_resolved) = judgment::scored_hold_totals_with_carry(
        inputs.rolls_held_for_score,
        inputs.rolls_let_go_for_score,
        carry.rolls_held_for_score,
        carry.rolls_let_go_for_score,
    );
    judgment::ExScoreData {
        counts: inputs.counts,
        counts_10ms: inputs.counts_10ms,
        holds_held,
        holds_resolved,
        rolls_held,
        rolls_resolved,
        mines_hit: inputs
            .mines_hit_for_score
            .saturating_add(carry.mines_hit_for_score),
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
    }
}

#[inline(always)]
pub fn effective_ex_score_inputs(
    live: ExScoreInputs,
    failed_snapshot: Option<ExScoreInputs>,
) -> ExScoreInputs {
    failed_snapshot.unwrap_or(live)
}

#[inline(always)]
pub fn capture_failed_ex_score_inputs(
    failed_snapshot: &mut Option<ExScoreInputs>,
    fail_time: Option<f32>,
    live: ExScoreInputs,
) -> bool {
    if fail_time.is_none() || failed_snapshot.is_some() {
        return false;
    }
    *failed_snapshot = Some(live);
    true
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ItgScoreStage {
    pub scoring_counts: judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ItgScoreInputs {
    pub scoring_counts: judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub rolls_held_for_score: u32,
    pub mines_hit_for_score: u32,
    pub holds_resolved_for_score: u32,
    pub rolls_resolved_for_score: u32,
    pub possible_grade_points: i32,
}

pub fn itg_score_inputs_from_display(
    stage: ItgScoreStage,
    carry: CourseDisplayCarry,
    totals: CourseDisplayTotals,
) -> ItgScoreInputs {
    let mut scoring_counts = stage.scoring_counts;
    let mut ix = 0usize;
    while ix < judgment::JUDGE_GRADE_COUNT {
        scoring_counts[ix] = scoring_counts[ix].saturating_add(carry.scoring_counts[ix]);
        ix += 1;
    }

    let holds_held_for_score = stage
        .holds_held_for_score
        .saturating_add(carry.holds_held_for_score);
    let rolls_held_for_score = stage
        .rolls_held_for_score
        .saturating_add(carry.rolls_held_for_score);
    ItgScoreInputs {
        scoring_counts,
        holds_held_for_score,
        rolls_held_for_score,
        mines_hit_for_score: stage
            .mines_hit_for_score
            .saturating_add(carry.mines_hit_for_score),
        holds_resolved_for_score: holds_held_for_score
            .saturating_add(stage.holds_let_go_for_score)
            .saturating_add(carry.holds_let_go_for_score),
        rolls_resolved_for_score: rolls_held_for_score
            .saturating_add(stage.rolls_let_go_for_score)
            .saturating_add(carry.rolls_let_go_for_score),
        possible_grade_points: totals.possible_grade_points,
    }
}

pub fn itg_score_percent_from_inputs(inputs: ItgScoreInputs) -> f64 {
    judgment::calculate_itg_score_percent_from_counts(
        &inputs.scoring_counts,
        inputs.holds_held_for_score,
        inputs.rolls_held_for_score,
        inputs.mines_hit_for_score,
        inputs.possible_grade_points,
    )
}

pub fn predictive_itg_score_percent_from_inputs(inputs: ItgScoreInputs) -> f64 {
    let actual = judgment::calculate_itg_grade_points_from_counts(
        &inputs.scoring_counts,
        inputs.holds_held_for_score,
        inputs.rolls_held_for_score,
        inputs.mines_hit_for_score,
    );
    let current_possible = judgment::current_possible_grade_points_from_counts(
        &inputs.scoring_counts,
        inputs.holds_resolved_for_score,
        inputs.rolls_resolved_for_score,
    );
    let (kept, _, _) = judgment::predictive_itg_score_percents(
        current_possible,
        inputs.possible_grade_points,
        actual,
    );
    kept
}

pub fn display_itg_score_percent_for_mode(
    inputs: ItgScoreInputs,
    mode: GameplayScoreDisplayMode,
) -> f64 {
    match mode {
        GameplayScoreDisplayMode::Normal => itg_score_percent_from_inputs(inputs) * 100.0,
        GameplayScoreDisplayMode::Predictive => predictive_itg_score_percent_from_inputs(inputs),
    }
}

pub fn display_ex_score_percent_for_mode(
    score: &judgment::ExScoreData,
    mode: GameplayScoreDisplayMode,
) -> f64 {
    match mode {
        GameplayScoreDisplayMode::Normal => judgment::ex_score_percent(score),
        GameplayScoreDisplayMode::Predictive => judgment::predictive_ex_score_percents(score).0,
    }
}

pub fn display_hard_ex_score_percent_for_mode(
    score: &judgment::ExScoreData,
    mode: GameplayScoreDisplayMode,
) -> f64 {
    match mode {
        GameplayScoreDisplayMode::Normal => judgment::hard_ex_score_percent(score),
        GameplayScoreDisplayMode::Predictive => {
            judgment::predictive_hard_ex_score_percents(score).0
        }
    }
}

#[inline(always)]
pub fn stream_segments_for_note_data(
    notes: &[u8],
    lanes: usize,
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    let densities = measure_densities(notes, lanes);
    zmod_stream_totals_for_densities(&densities, constant_bpm)
}

pub fn measure_counter_segments_for_densities(
    densities: &[usize],
    notes_threshold: Option<usize>,
) -> Vec<StreamSegment> {
    notes_threshold.map_or_else(Vec::new, |threshold| {
        stream_sequences_threshold(densities, threshold)
    })
}

#[inline(always)]
pub fn zmod_stream_totals_for_densities(
    densities: &[usize],
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    zmod_stream_totals_full_measures(densities, constant_bpm)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTargetScoreSetting {
    CMinus,
    C,
    CPlus,
    BMinus,
    B,
    BPlus,
    AMinus,
    A,
    APlus,
    SMinus,
    #[default]
    S,
    SPlus,
    MachineBest,
    PersonalBest,
}

pub const fn target_score_setting_percent(setting: GameplayTargetScoreSetting) -> Option<f64> {
    match setting {
        GameplayTargetScoreSetting::CMinus => Some(50.0),
        GameplayTargetScoreSetting::C => Some(55.0),
        GameplayTargetScoreSetting::CPlus => Some(60.0),
        GameplayTargetScoreSetting::BMinus => Some(64.0),
        GameplayTargetScoreSetting::B => Some(68.0),
        GameplayTargetScoreSetting::BPlus => Some(72.0),
        GameplayTargetScoreSetting::AMinus => Some(76.0),
        GameplayTargetScoreSetting::A => Some(80.0),
        GameplayTargetScoreSetting::APlus => Some(83.0),
        GameplayTargetScoreSetting::SMinus => Some(86.0),
        GameplayTargetScoreSetting::S => Some(89.0),
        GameplayTargetScoreSetting::SPlus => Some(92.0),
        GameplayTargetScoreSetting::MachineBest | GameplayTargetScoreSetting::PersonalBest => None,
    }
}

pub fn resolve_target_score_percent(
    setting: GameplayTargetScoreSetting,
    personal_best: Option<f64>,
    machine_best: Option<f64>,
) -> f64 {
    match setting {
        GameplayTargetScoreSetting::MachineBest => machine_best.or(personal_best),
        GameplayTargetScoreSetting::PersonalBest => personal_best,
        fixed => target_score_setting_percent(fixed),
    }
    .unwrap_or(89.0)
}

