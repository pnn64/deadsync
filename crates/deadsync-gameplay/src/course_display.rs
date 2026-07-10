#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayTotals {
    pub possible_grade_points: i32,
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
}

pub fn course_display_totals_for_chart(chart: &ChartData) -> CourseDisplayTotals {
    CourseDisplayTotals {
        possible_grade_points: chart.possible_grade_points,
        total_steps: chart.stats.total_steps,
        holds_total: chart.holds_total,
        rolls_total: chart.rolls_total,
        mines_total: chart.mines_total,
    }
}

pub fn course_display_totals_for_player(
    totals: Option<&[CourseDisplayTotals; MAX_PLAYERS]>,
    possible_grade_points: &[i32; MAX_PLAYERS],
    total_steps: &[u32; MAX_PLAYERS],
    holds_total: &[u32; MAX_PLAYERS],
    rolls_total: &[u32; MAX_PLAYERS],
    mines_total: &[u32; MAX_PLAYERS],
    player_idx: usize,
) -> CourseDisplayTotals {
    if player_idx >= MAX_PLAYERS {
        return CourseDisplayTotals::default();
    }
    if let Some(totals) = totals {
        return totals[player_idx];
    }
    CourseDisplayTotals {
        possible_grade_points: possible_grade_points[player_idx],
        total_steps: total_steps[player_idx],
        holds_total: holds_total[player_idx],
        rolls_total: rolls_total[player_idx],
        mines_total: mines_total[player_idx],
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayCarry {
    // ITGmania keeps the same lifemeter alive between nonstop course songs.
    pub life: f32,
    pub judgment_counts: [u32; 6],
    pub scoring_counts: [u32; 6],
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: WindowCounts,
    pub first_fc_attempt_broken: bool,
    // Canonical FA+ split (15ms) used for EX scoring/evaluation.
    pub window_counts: WindowCounts,
    // Canonical 10ms split used for H.EX scoring/evaluation.
    pub window_counts_10ms_blue: WindowCounts,
    // Display split used by gameplay counters (legacy 10ms or custom ms option).
    pub window_counts_display_blue: WindowCounts,
    pub holds_held: u32,
    pub rolls_held: u32,
    pub mines_avoided: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseComboCarryState {
    pub combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: WindowCounts,
    pub first_fc_attempt_broken: bool,
}

pub fn course_life_after_carry(current_life: f32, course_carry: Option<CourseDisplayCarry>) -> f32 {
    let Some(carry) = course_carry else {
        return current_life;
    };
    if carry.life.is_finite() {
        carry.life.clamp(0.0, 1.0)
    } else {
        current_life
    }
}

pub fn apply_course_combo_carry_state(
    state: &mut CourseComboCarryState,
    carry_combo_between_songs: bool,
    replay_mode: bool,
    combo_carry: u32,
    course_carry: Option<CourseDisplayCarry>,
) {
    if carry_combo_between_songs && !replay_mode {
        state.combo = combo_carry;
        if let Some(carry) = course_carry {
            if combo_carry > 0 {
                state.full_combo_grade = carry.full_combo_grade;
                state.current_combo_grade = carry.current_combo_grade;
                state.current_combo_window_counts = carry.current_combo_window_counts;
                state.first_fc_attempt_broken = carry.first_fc_attempt_broken;
            } else {
                state.first_fc_attempt_broken =
                    carry.first_fc_attempt_broken || carry.full_combo_grade.is_some();
            }
        }
    } else if course_carry.is_some() {
        state.first_fc_attempt_broken = true;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayWindowCountsSources {
    pub canonical: WindowCounts,
    pub ten_ms_blue: WindowCounts,
    pub display_blue: WindowCounts,
}

impl Default for DisplayWindowCountsSources {
    fn default() -> Self {
        Self {
            canonical: WindowCounts::default(),
            ten_ms_blue: WindowCounts::default(),
            display_blue: WindowCounts::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayWindowCountsState {
    pub canonical: [WindowCounts; MAX_PLAYERS],
    pub ten_ms_blue: [WindowCounts; MAX_PLAYERS],
    pub display_blue: [WindowCounts; MAX_PLAYERS],
}

impl GameplayWindowCountsState {
    #[inline(always)]
    pub fn sources(&self, player: usize) -> DisplayWindowCountsSources {
        DisplayWindowCountsSources {
            canonical: self.canonical(player),
            ten_ms_blue: self.ten_ms_blue(player),
            display_blue: self.display_blue(player),
        }
    }

    #[inline(always)]
    pub fn canonical(&self, player: usize) -> WindowCounts {
        self.canonical.get(player).copied().unwrap_or_default()
    }

    #[inline(always)]
    pub fn ten_ms_blue(&self, player: usize) -> WindowCounts {
        self.ten_ms_blue.get(player).copied().unwrap_or_default()
    }

    #[inline(always)]
    pub fn display_blue(&self, player: usize) -> WindowCounts {
        self.display_blue.get(player).copied().unwrap_or_default()
    }

    #[inline(always)]
    pub fn record_judgment(
        &mut self,
        player: usize,
        judgment: &Judgment,
        display_blue_window_ms: f32,
    ) {
        if player >= MAX_PLAYERS {
            return;
        }
        record_display_window_counts_for_judgment(
            &mut self.canonical[player],
            &mut self.ten_ms_blue[player],
            &mut self.display_blue[player],
            judgment,
            display_blue_window_ms,
        );
    }

    #[inline(always)]
    pub fn set_player_for_benchmark(
        &mut self,
        player: usize,
        canonical: WindowCounts,
        ten_ms_blue: WindowCounts,
        display_blue: WindowCounts,
    ) {
        if player >= MAX_PLAYERS {
            return;
        }
        self.canonical[player] = canonical;
        self.ten_ms_blue[player] = ten_ms_blue;
        self.display_blue[player] = display_blue;
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DisplayWindowCountsMode {
    Canonical,
    TenMsBlue,
    DisplayBlue,
    CustomBlue { split_ms: f32 },
}

impl Default for DisplayWindowCountsMode {
    fn default() -> Self {
        Self::Canonical
    }
}

#[inline(always)]
fn display_float_match(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.000_1
}

pub fn display_window_counts_mode(
    blue_window_ms: Option<f32>,
    display_blue_window_ms: f32,
) -> DisplayWindowCountsMode {
    let Some(ms) = blue_window_ms else {
        return DisplayWindowCountsMode::Canonical;
    };
    let split_ms = judgment::normalized_blue_window_ms(ms);
    let display_split_ms = judgment::normalized_blue_window_ms(display_blue_window_ms);
    if display_float_match(split_ms, FA_PLUS_W0_MS) {
        DisplayWindowCountsMode::Canonical
    } else if display_float_match(split_ms, FA_PLUS_W010_MS) {
        DisplayWindowCountsMode::TenMsBlue
    } else if display_float_match(split_ms, display_split_ms) {
        DisplayWindowCountsMode::DisplayBlue
    } else {
        DisplayWindowCountsMode::CustomBlue { split_ms }
    }
}

pub fn display_window_counts_current(
    sources: DisplayWindowCountsSources,
    mode: DisplayWindowCountsMode,
) -> Option<WindowCounts> {
    match mode {
        DisplayWindowCountsMode::Canonical => Some(sources.canonical),
        DisplayWindowCountsMode::TenMsBlue => Some(sources.ten_ms_blue),
        DisplayWindowCountsMode::DisplayBlue => Some(sources.display_blue),
        DisplayWindowCountsMode::CustomBlue { .. } => None,
    }
}

pub fn display_window_counts_with_carry(
    current: WindowCounts,
    carry: CourseDisplayCarry,
    mode: DisplayWindowCountsMode,
) -> WindowCounts {
    let carry_counts = match mode {
        DisplayWindowCountsMode::Canonical => carry.window_counts,
        DisplayWindowCountsMode::TenMsBlue => carry.window_counts_10ms_blue,
        DisplayWindowCountsMode::DisplayBlue | DisplayWindowCountsMode::CustomBlue { .. } => {
            carry.window_counts_display_blue
        }
    };
    judgment::add_window_counts(current, carry_counts)
}

pub fn display_window_counts_for_notes(
    sources: DisplayWindowCountsSources,
    carry: CourseDisplayCarry,
    notes: &[Note],
    blue_window_ms: Option<f32>,
    display_blue_window_ms: f32,
) -> WindowCounts {
    let mode = display_window_counts_mode(blue_window_ms, display_blue_window_ms);
    let current = match display_window_counts_current(sources, mode) {
        Some(counts) => counts,
        None => {
            let DisplayWindowCountsMode::CustomBlue { split_ms } = mode else {
                return WindowCounts::default();
            };
            deadsync_rules::timing::compute_window_counts_blue_ms(notes, split_ms)
        }
    };
    display_window_counts_with_carry(current, carry, mode)
}

pub fn record_display_window_counts_for_judgment(
    canonical: &mut WindowCounts,
    ten_ms_blue: &mut WindowCounts,
    display_blue: &mut WindowCounts,
    judgment: &Judgment,
    display_blue_window_ms: f32,
) {
    judgment::add_judgment_to_window_counts(canonical, judgment, FA_PLUS_W0_MS);
    judgment::add_judgment_to_window_counts(ten_ms_blue, judgment, FA_PLUS_W010_MS);
    judgment::add_judgment_to_window_counts(display_blue, judgment, display_blue_window_ms);
}

pub fn record_combo_window_count_for_judgment(counts: &mut WindowCounts, judgment: &Judgment) {
    judgment::add_judgment_to_window_counts(counts, judgment, FA_PLUS_W0_MS);
}

pub fn display_judgment_count_for_grade(
    stage_counts: judgment::JudgeCounts,
    carry: CourseDisplayCarry,
    grade: JudgeGrade,
) -> u32 {
    let ix = judgment::display_judge_ix(grade);
    stage_counts[ix].saturating_add(carry.judgment_counts[ix])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayScoreDisplayMode {
    #[default]
    Normal,
    Predictive,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayStage {
    pub life: f32,
    pub judgment_counts: judgment::JudgeCounts,
    pub scoring_counts: judgment::JudgeCounts,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: WindowCounts,
    pub combo: u32,
    pub first_fc_attempt_broken: bool,
    pub window_counts: WindowCounts,
    pub window_counts_10ms_blue: WindowCounts,
    pub window_counts_display_blue: WindowCounts,
    pub holds_held: u32,
    pub rolls_held: u32,
    pub mines_avoided: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

pub fn course_display_carry_for_stage(
    previous: CourseDisplayCarry,
    stage: CourseDisplayStage,
) -> CourseDisplayCarry {
    let mut judgment_counts = [0u32; judgment::JUDGE_GRADE_COUNT];
    let mut scoring_counts = [0u32; judgment::JUDGE_GRADE_COUNT];
    let mut ix = 0usize;
    while ix < judgment::JUDGE_GRADE_COUNT {
        judgment_counts[ix] =
            previous.judgment_counts[ix].saturating_add(stage.judgment_counts[ix]);
        scoring_counts[ix] = previous.scoring_counts[ix].saturating_add(stage.scoring_counts[ix]);
        ix += 1;
    }

    let first_fc_attempt_broken = previous.first_fc_attempt_broken || stage.first_fc_attempt_broken;
    let full_combo_grade = if first_fc_attempt_broken {
        None
    } else {
        match (previous.full_combo_grade, stage.full_combo_grade) {
            (Some(prev), Some(current)) => Some(prev.max(current)),
            (Some(prev), None) => Some(prev),
            (None, current) => current,
        }
    };

    CourseDisplayCarry {
        life: stage.life.clamp(0.0, 1.0),
        judgment_counts,
        scoring_counts,
        full_combo_grade,
        current_combo_grade: stage.current_combo_grade,
        current_combo_window_counts: if stage.combo > 0 {
            stage.current_combo_window_counts
        } else {
            WindowCounts::default()
        },
        first_fc_attempt_broken,
        window_counts: judgment::add_window_counts(previous.window_counts, stage.window_counts),
        window_counts_10ms_blue: judgment::add_window_counts(
            previous.window_counts_10ms_blue,
            stage.window_counts_10ms_blue,
        ),
        window_counts_display_blue: judgment::add_window_counts(
            previous.window_counts_display_blue,
            stage.window_counts_display_blue,
        ),
        holds_held: previous.holds_held.saturating_add(stage.holds_held),
        rolls_held: previous.rolls_held.saturating_add(stage.rolls_held),
        mines_avoided: previous.mines_avoided.saturating_add(stage.mines_avoided),
        holds_held_for_score: previous
            .holds_held_for_score
            .saturating_add(stage.holds_held_for_score),
        holds_let_go_for_score: previous
            .holds_let_go_for_score
            .saturating_add(stage.holds_let_go_for_score),
        rolls_held_for_score: previous
            .rolls_held_for_score
            .saturating_add(stage.rolls_held_for_score),
        rolls_let_go_for_score: previous
            .rolls_let_go_for_score
            .saturating_add(stage.rolls_let_go_for_score),
        mines_hit_for_score: previous
            .mines_hit_for_score
            .saturating_add(stage.mines_hit_for_score),
    }
}

pub fn course_display_carry_for_stages(
    previous: Option<&[CourseDisplayCarry; MAX_PLAYERS]>,
    stages: [CourseDisplayStage; MAX_PLAYERS],
    num_players: usize,
) -> [CourseDisplayCarry; MAX_PLAYERS] {
    let mut carry = [CourseDisplayCarry::default(); MAX_PLAYERS];
    for player in 0..num_players.min(MAX_PLAYERS) {
        let previous_player = previous.map_or(CourseDisplayCarry::default(), |old| old[player]);
        carry[player] = course_display_carry_for_stage(previous_player, stages[player]);
    }
    if num_players == 1 {
        carry[1] = carry[0];
    }
    carry
}

pub fn course_display_carry_for_player(
    carry: Option<&[CourseDisplayCarry; MAX_PLAYERS]>,
    player_idx: usize,
) -> CourseDisplayCarry {
    if player_idx >= MAX_PLAYERS {
        return CourseDisplayCarry::default();
    }
    carry.map_or(CourseDisplayCarry::default(), |carry| carry[player_idx])
}

pub fn player_course_display_stage(
    player: &PlayerRuntime,
    window_counts: WindowCounts,
    window_counts_10ms_blue: WindowCounts,
    window_counts_display_blue: WindowCounts,
) -> CourseDisplayStage {
    CourseDisplayStage {
        life: player.life,
        judgment_counts: player.judgment_counts,
        scoring_counts: player.scoring_counts,
        full_combo_grade: player.full_combo_grade,
        current_combo_grade: player.current_combo_grade,
        current_combo_window_counts: player.current_combo_window_counts,
        combo: player.combo,
        first_fc_attempt_broken: player.first_fc_attempt_broken,
        window_counts,
        window_counts_10ms_blue,
        window_counts_display_blue,
        holds_held: player.holds_held,
        rolls_held: player.rolls_held,
        mines_avoided: player.mines_avoided,
        holds_held_for_score: player.holds_held_for_score,
        holds_let_go_for_score: player.holds_let_go_for_score,
        rolls_held_for_score: player.rolls_held_for_score,
        rolls_let_go_for_score: player.rolls_let_go_for_score,
        mines_hit_for_score: player.mines_hit_for_score,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayTiming {
    pub elapsed_seconds: f32,
    pub total_seconds: f32,
}

pub fn course_display_timing_for_stages<T>(
    stages: &[T],
    next_stage_index: usize,
    music_seconds: impl Fn(&T) -> f32,
) -> CourseDisplayTiming {
    let mut elapsed_seconds = 0.0;
    let mut total_seconds = 0.0;
    for (idx, stage) in stages.iter().enumerate() {
        let seconds = sanitize_course_display_seconds(music_seconds(stage));
        if idx < next_stage_index {
            elapsed_seconds += seconds;
        }
        total_seconds += seconds;
    }
    CourseDisplayTiming {
        elapsed_seconds,
        total_seconds,
    }
}

#[inline(always)]
fn sanitize_course_display_seconds(seconds: f32) -> f32 {
    if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayCourseDisplayState {
    carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    timing: Option<CourseDisplayTiming>,
}

impl GameplayCourseDisplayState {
    #[inline(always)]
    pub fn new(
        carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
        totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
        timing: Option<CourseDisplayTiming>,
    ) -> Self {
        Self {
            carry,
            totals,
            timing,
        }
    }

    #[inline(always)]
    pub fn is_course_stage(&self) -> bool {
        self.totals.is_some()
    }

    #[inline(always)]
    pub fn carry(&self) -> Option<&[CourseDisplayCarry; MAX_PLAYERS]> {
        self.carry.as_ref()
    }

    #[inline(always)]
    pub fn totals(&self) -> Option<&[CourseDisplayTotals; MAX_PLAYERS]> {
        self.totals.as_ref()
    }

    #[inline(always)]
    pub fn timing(&self) -> Option<CourseDisplayTiming> {
        self.timing
    }

    #[inline(always)]
    pub fn carry_for_player(&self, player_idx: usize) -> CourseDisplayCarry {
        course_display_carry_for_player(self.carry(), player_idx)
    }
}
