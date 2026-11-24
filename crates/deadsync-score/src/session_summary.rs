//! Aggregates recent passed stages for a session result page.

use crate::{Grade, score_to_grade, stage_stats::StageSummary};
use deadsync_rules::judgment::{JudgeCounts, calculate_itg_score_percent_from_points};

#[derive(Clone, Debug)]
pub struct SessionPlayerSummary {
    pub judgments: JudgeCounts,
    pub grade: Grade,
    pub percent: f64,
    pub numeric_score: u32,
    /// Sum of the selected stages' best combos.
    pub combo_total: u32,
    pub holds: u32,
    pub rolls: u32,
    pub radar_possible: [f32; 5],
    pub radar_achievement: Option<[f32; 5]>,
}

#[derive(Clone, Debug, Default)]
pub struct SessionSummary {
    /// Newest first, matching the aggregation and banner drawing order.
    pub stage_indices: Vec<usize>,
    pub players: [Option<SessionPlayerSummary>; 2],
}

pub fn recent_passed(stages: &[StageSummary], limit: usize) -> SessionSummary {
    let indices: Vec<_> = stages
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, stage)| {
            stage
                .players
                .iter()
                .flatten()
                .any(|player| player.grade != Grade::Failed)
        })
        .take(limit)
        .map(|(index, _)| index)
        .collect();
    let players = std::array::from_fn(|side| {
        let mut judgments = [0u32; 6];
        let mut score = 0u32;
        let mut combo = 0u32;
        let mut holds = 0u32;
        let mut rolls = 0u32;
        let mut earned = 0i32;
        let mut possible = 0i32;
        let mut failed = false;
        let mut found = false;
        let mut radar_possible = [0.0; 5];
        let mut radar_actual = Some([0.0; 5]);
        for &index in &indices {
            let stage = &stages[index];
            let Some(player) = stage.players[side].as_ref() else {
                continue;
            };
            found = true;
            for (target, value) in judgments.iter_mut().zip(player.judgment_counts) {
                *target = target.saturating_add(value);
            }
            score = score.saturating_add(player.numeric_score);
            combo = combo.saturating_add(player.max_combo);
            holds = holds.saturating_add(player.holds_held);
            rolls = rolls.saturating_add(player.rolls_held);
            earned = earned.saturating_add(player.earned_grade_points);
            possible = possible.saturating_add(player.possible_grade_points);
            failed |= player.grade == Grade::Failed;
            let chart_radar = player.chart.radar_values(stage.song.music_length_seconds);
            for axis in 0..5 {
                radar_possible[axis] += chart_radar[axis];
            }
            radar_actual = radar_actual
                .zip(player.radar_achievement)
                .map(|(total, values)| std::array::from_fn(|axis| total[axis] + values[axis]));
        }
        if !found {
            return None;
        }
        let count = indices.len().max(1) as f32;
        let percent = calculate_itg_score_percent_from_points(earned, possible);
        Some(SessionPlayerSummary {
            judgments,
            grade: if failed {
                Grade::Failed
            } else {
                score_to_grade(percent * 10000.0)
            },
            percent,
            numeric_score: score,
            combo_total: combo,
            holds,
            rolls,
            radar_possible: radar_possible.map(|value| value / count),
            radar_achievement: radar_actual.map(|values| values.map(|value| value / count)),
        })
    });
    SessionSummary {
        stage_indices: indices,
        players,
    }
}
