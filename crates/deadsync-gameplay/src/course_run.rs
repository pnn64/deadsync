//! Active course preparation and entry shared by gameplay and presentation.

use std::path::PathBuf;
use std::sync::Arc;

use deadsync_chart::course::{CourseCyclePlan, CourseSessionPlan, CourseStagePlan};
use deadsync_chart::{GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_rules::course::CourseType;

use crate::course_progress::{CourseProgress, CourseRenewal};
use crate::{
    CourseDisplayTiming, CourseDisplayTotals, CourseLifeConfig, CourseLifeStage, LeadInTiming,
    course_display_timing_for_stages, course_display_totals_for_chart,
    course_display_totals_for_chart_side, course_life_config,
};

#[derive(Clone)]
pub struct CourseStageRuntime {
    pub song: Arc<SongData>,
    pub steps_index: [usize; MAX_PLAYERS],
    pub preferred_difficulty_index: [usize; MAX_PLAYERS],
    pub modifiers: String,
    pub gain_seconds: f32,
    pub gain_lives: i32,
}

#[derive(Clone)]
pub struct CourseRun {
    pub path: PathBuf,
    pub score_hash: String,
    pub course_difficulty_name: String,
    pub course_meter: Option<u32>,
    pub course_type: CourseType,
    pub lives: i32,
    pub stages: Vec<CourseStageRuntime>,
    pub course_display_totals: [CourseDisplayTotals; MAX_PLAYERS],
    pub progress: CourseProgress,
}

impl CourseRun {
    /// Resolve an ordered course plan against the current chart catalog.
    /// A failed partner-chart load rejects the entire run.
    pub fn prepare(
        selection: CourseSessionPlan,
        chart_type: &str,
        mut load: impl FnMut(&SongData, &[usize], f32) -> Result<Vec<GameplayChartData>, String>,
    ) -> Result<Option<Self>, String> {
        let mut run = Self {
            path: selection.path,
            score_hash: selection.score_hash,
            course_difficulty_name: selection.course_difficulty_name,
            course_meter: selection.course_meter,
            course_type: selection.course_type,
            lives: selection.lives,
            stages: Vec::with_capacity(selection.stages.len()),
            course_display_totals: [CourseDisplayTotals::default(); MAX_PLAYERS],
            progress: CourseProgress::default(),
        };
        let prepared = run.append_stages(selection.stages, chart_type, &mut load)?;
        Ok(prepared.then_some(run))
    }

    /// Append the resolved stages of a new Endless cycle as one operation.
    pub fn append_endless_cycle(
        &mut self,
        selection: CourseCyclePlan,
        chart_type: &str,
        mut load: impl FnMut(&SongData, &[usize], f32) -> Result<Vec<GameplayChartData>, String>,
    ) -> Result<bool, String> {
        if !self
            .progress
            .can_append_endless_cycle(self.course_type == CourseType::Endless)
            || selection.path != self.path
        {
            return Ok(false);
        }
        self.append_stages(selection.stages, chart_type, &mut load)
    }

    fn append_stages(
        &mut self,
        stages: Vec<CourseStagePlan>,
        chart_type: &str,
        load: &mut impl FnMut(&SongData, &[usize], f32) -> Result<Vec<GameplayChartData>, String>,
    ) -> Result<bool, String> {
        let previous_len = self.stages.len();
        let mut totals = self.course_display_totals;
        for stage in stages
            .into_iter()
            .filter_map(|plan| course_stage_runtime_from_plan(plan, chart_type))
        {
            if let Err(error) =
                add_course_stage_totals_with_loader(&mut totals, &stage, chart_type, load)
            {
                self.stages.truncate(previous_len);
                return Err(error);
            }
            self.stages.push(stage);
        }
        self.course_display_totals = totals;
        Ok(self.stages.len() != previous_len)
    }

    pub fn has_next_stage(&self) -> bool {
        self.progress
            .has_next_stage(self.stages.len(), self.course_type == CourseType::Endless)
    }

    pub fn next_stage(&self) -> Option<&CourseStageRuntime> {
        self.entry(false).next_stage()
    }

    pub fn is_finished(&self, failed: bool) -> bool {
        self.progress.is_finished(
            self.stages.len(),
            self.course_type == CourseType::Endless,
            failed,
        )
    }

    pub fn renewal(&self) -> CourseRenewal {
        self.progress
            .renewal(self.stages.len(), self.course_type == CourseType::Endless)
    }

    /// Read the upcoming stage, or the current stage for a Lua replacement.
    pub fn entry(&self, lua_reload: bool) -> CourseEntry<'_> {
        CourseEntry {
            run: self,
            progress: self.progress.for_entry(lua_reload),
        }
    }

    pub fn total_seconds(&self) -> f32 {
        self.stages.iter().map(course_stage_seconds).sum()
    }
}

/// An entry-specific cursor over the existing stage sequence.
#[derive(Clone, Copy)]
pub struct CourseEntry<'a> {
    run: &'a CourseRun,
    progress: CourseProgress,
}

impl<'a> CourseEntry<'a> {
    pub fn next_stage(self) -> Option<&'a CourseStageRuntime> {
        let index = self.progress.stage_index(
            self.run.stages.len(),
            self.run.course_type == CourseType::Endless,
        )?;
        self.run.stages.get(index)
    }

    pub fn display_timing(self) -> CourseDisplayTiming {
        course_display_timing_for_stages(
            &self.run.stages,
            self.progress.next_stage_index(),
            |stage| stage.song.music_length_seconds,
        )
    }

    pub fn life_config(self, chart_type: &str) -> [CourseLifeConfig; MAX_PLAYERS] {
        let stage = self.next_stage().map(|stage| CourseLifeStage {
            song: stage.song.as_ref(),
            steps_index: stage.steps_index,
            gain_lives: stage.gain_lives,
            gain_seconds: stage.gain_seconds,
        });
        course_life_config(self.run.course_type, self.run.lives, stage, chart_type)
    }

    pub fn lead_in_timing(self) -> Option<LeadInTiming> {
        self.progress
            .uses_next_stage_lead_in()
            .then_some(LeadInTiming {
                // ITGmania uses a shorter lead-in for each subsequent course song.
                min_seconds_to_step: 4.0,
                min_seconds_to_music: 0.0,
            })
    }

    pub fn stage_number(self) -> usize {
        self.progress.next_stage_index().saturating_add(1)
    }

    pub fn finite_stage_count(self) -> Option<usize> {
        (self.run.course_type != CourseType::Endless).then_some(self.run.stages.len())
    }
}

fn course_stage_runtime_from_plan(
    plan: CourseStagePlan,
    chart_type: &str,
) -> Option<CourseStageRuntime> {
    let steps_idx = plan
        .song
        .steps_index_for_chart_hash(chart_type, plan.chart_hash.as_str())?;
    Some(CourseStageRuntime {
        song: plan.song,
        steps_index: [steps_idx; MAX_PLAYERS],
        preferred_difficulty_index: [steps_idx; MAX_PLAYERS],
        modifiers: plan.modifiers,
        gain_seconds: plan.gain_seconds,
        gain_lives: plan.gain_lives,
    })
}

/// Loading can fail if a simfile disappears or changes after course selection.
/// Keep the loader explicit so this boundary can be exercised without global
/// scan configuration or external song packs.
fn add_course_stage_totals_with_loader(
    totals: &mut [CourseDisplayTotals; MAX_PLAYERS],
    stage: &CourseStageRuntime,
    chart_type: &str,
    load: &mut impl FnMut(&SongData, &[usize], f32) -> Result<Vec<GameplayChartData>, String>,
) -> Result<(), String> {
    use crate::GameplayInputPlayStyle as Style;
    let partner_style = [
        ("dance-couple", Style::Couple),
        ("pump-couple", Style::PumpCouple),
        ("dance-routine", Style::Routine),
        ("pump-routine", Style::PumpRoutine),
    ]
    .into_iter()
    .find_map(|(name, style)| name.eq_ignore_ascii_case(chart_type).then_some(style));
    let mut updated = *totals;
    let mut loaded: Option<(usize, GameplayChartData)> = None;
    for (player_idx, total) in updated.iter_mut().enumerate() {
        let steps_index = stage.steps_index[player_idx];
        let context = || {
            format!(
                "{}: {chart_type} chart at steps index {steps_index}",
                stage.song.simfile_path.display()
            )
        };
        let chart = stage
            .song
            .chart_for_steps_index(chart_type, steps_index)
            .ok_or_else(|| format!("{} is missing", context()))?;
        let add = if let Some(style) = partner_style {
            let index = stage
                .song
                .charts
                .iter()
                .position(|entry| std::ptr::eq(entry, chart))
                .ok_or_else(|| format!("{} is not in its song", context()))?;
            if loaded
                .as_ref()
                .is_none_or(|(loaded_index, _)| *loaded_index != index)
            {
                let mut payloads = load(&stage.song, &[index], 0.0)
                    .map_err(|error| format!("Cannot load {}: {error}", context()))?;
                if payloads.len() != 1 {
                    return Err(format!(
                        "Loader returned {} payloads for {} (expected one)",
                        payloads.len(),
                        context()
                    ));
                }
                loaded = payloads.pop().map(|payload| (index, payload));
            }
            let (_, payload) = loaded.as_ref().expect("one validated payload was loaded");
            course_display_totals_for_chart_side(chart, payload, style, player_idx)
        } else {
            course_display_totals_for_chart(chart)
        };
        total.possible_grade_points = total
            .possible_grade_points
            .saturating_add(add.possible_grade_points);
        total.total_steps = total.total_steps.saturating_add(add.total_steps);
        total.holds_total = total.holds_total.saturating_add(add.holds_total);
        total.rolls_total = total.rolls_total.saturating_add(add.rolls_total);
        total.mines_total = total.mines_total.saturating_add(add.mines_total);
    }
    *totals = updated;
    Ok(())
}

fn course_stage_seconds(stage: &CourseStageRuntime) -> f32 {
    let seconds = stage.song.precise_last_second();
    if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
#[path = "course_run_tests.rs"]
mod tests;
