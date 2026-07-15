use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use deadsync_score::stage_stats::{PlayerStageSummary, StageSummary};
use deadsync_theme_simply_love::views::ScoreInfo;
use std::borrow::Cow;
use std::ops::Range;

fn notes_hit(score: &ScoreInfo) -> u32 {
    score.column_judgments.iter().fold(0, |total, column| {
        total
            .saturating_add(column.w0)
            .saturating_add(column.w1)
            .saturating_add(column.w2)
            .saturating_add(column.w3)
            .saturating_add(column.w4)
            .saturating_add(column.w5)
    })
}

fn player_stage_summary(score: &ScoreInfo) -> PlayerStageSummary {
    PlayerStageSummary {
        profile_name: score.profile_name.clone(),
        chart: score.chart.clone(),
        score_valid: score.score_valid,
        disqualified: score.disqualified,
        groovestats: score.groovestats.clone(),
        itl: score.itl.clone(),
        grade: score.grade,
        score_percent: score.score_percent,
        earned_grade_points: score.earned_grade_points,
        possible_grade_points: score.possible_grade_points,
        ex_score_percent: score.ex_score_percent,
        hard_ex_score_percent: score.hard_ex_score_percent,
        hands_achieved: score.hands_achieved,
        hands_total: score.hands_total,
        holds_held: score.holds_held,
        holds_held_for_score: score.holds_held_for_score,
        holds_total: score.holds_total,
        rolls_held: score.rolls_held,
        rolls_held_for_score: score.rolls_held_for_score,
        rolls_total: score.rolls_total,
        mines_hit_for_score: score.mines_hit_for_score,
        mines_avoided: score.mines_avoided,
        mines_total: score.mines_total,
        notes_hit: notes_hit(score),
        calories_burned: score.calories_burned,
        window_counts: score.window_counts,
        window_counts_10ms: score.window_counts_10ms,
        timing: score.timing,
        arrow_timing: score.arrow_timing.clone(),
        scatter: score.scatter.clone(),
        scatter_worst_window_ms: score.scatter_worst_window_ms,
        histogram: score.histogram.clone(),
        graph_first_second: score.graph_first_second,
        graph_last_second: score.graph_last_second,
        life_history: score.life_history.clone(),
        fail_time: score.fail_time,
        show_w0: (score.show_fa_plus_window && score.show_fa_plus_pane) || score.show_ex_score,
        show_fa_plus_pane: score.show_fa_plus_pane,
        show_ex_score: score.show_ex_score,
        show_hard_ex_score: score.show_hard_ex_score,
        track_early_judgments: score.track_early_judgments,
        dim_post_fail_scatter: score.dim_post_fail_scatter,
    }
}

pub fn stage_summary_from_score_info(
    score_info: &[Option<ScoreInfo>; MAX_PLAYERS],
    duration_seconds: f32,
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> Option<StageSummary> {
    let mut song = None;
    let mut music_rate = 1.0;
    let mut players: [Option<PlayerStageSummary>; MAX_PLAYERS] = std::array::from_fn(|_| None);

    match play_style {
        PlayStyle::Versus => {
            for (idx, side) in [(0, PlayerSide::P1), (1, PlayerSide::P2)] {
                let Some(score) = score_info.get(idx).and_then(|entry| entry.as_ref()) else {
                    continue;
                };
                song = Some(score.song.clone());
                music_rate = score.music_rate;
                players[player_side_index(side)] = Some(player_stage_summary(score));
            }
        }
        PlayStyle::Single | PlayStyle::Double => {
            let score = score_info.first().and_then(|entry| entry.as_ref())?;
            song = Some(score.song.clone());
            music_rate = score.music_rate;
            players[player_side_index(player_side)] = Some(player_stage_summary(score));
        }
    }

    Some(StageSummary {
        song: song?,
        music_rate: if music_rate.is_finite() && music_rate > 0.0 {
            music_rate
        } else {
            1.0
        },
        duration_seconds,
        players,
    })
}

pub fn post_select_display_stages<'a>(
    stages: &'a [StageSummary],
    hidden_indices: &[usize],
    show_course_individual_scores: bool,
) -> Cow<'a, [StageSummary]> {
    if show_course_individual_scores || hidden_indices.is_empty() || stages.is_empty() {
        return Cow::Borrowed(stages);
    }

    if let Some(range) = contiguous_visible_stage_range(stages.len(), hidden_indices) {
        return Cow::Borrowed(&stages[range]);
    }

    let mut filtered = Vec::with_capacity(stages.len().saturating_sub(hidden_indices.len()));
    let mut hidden_idx = 0usize;
    for (idx, stage) in stages.iter().enumerate() {
        while hidden_idx < hidden_indices.len() && hidden_indices[hidden_idx] < idx {
            hidden_idx = hidden_idx.saturating_add(1);
        }
        if hidden_idx < hidden_indices.len() && hidden_indices[hidden_idx] == idx {
            continue;
        }
        filtered.push(stage.clone());
    }
    Cow::Owned(filtered)
}

pub fn post_select_display_stage_count(
    stage_count: usize,
    hidden_indices: &[usize],
    show_course_individual_scores: bool,
) -> usize {
    if show_course_individual_scores || hidden_indices.is_empty() || stage_count == 0 {
        return stage_count;
    }

    let mut visible = stage_count;
    let mut hidden_idx = 0usize;
    for idx in 0..stage_count {
        while hidden_idx < hidden_indices.len() && hidden_indices[hidden_idx] < idx {
            hidden_idx += 1;
        }
        if hidden_idx < hidden_indices.len() && hidden_indices[hidden_idx] == idx {
            visible -= 1;
        }
    }
    visible
}

fn contiguous_visible_stage_range(
    stage_count: usize,
    hidden_indices: &[usize],
) -> Option<Range<usize>> {
    let mut hidden_idx = 0usize;
    let mut visible_start = None;
    let mut visible_end = 0usize;
    let mut hidden_after_visible = false;

    for idx in 0..stage_count {
        while hidden_idx < hidden_indices.len() && hidden_indices[hidden_idx] < idx {
            hidden_idx += 1;
        }
        let hidden = hidden_idx < hidden_indices.len() && hidden_indices[hidden_idx] == idx;
        if hidden {
            hidden_after_visible |= visible_start.is_some();
            continue;
        }
        if hidden_after_visible {
            return None;
        }
        visible_start.get_or_insert(idx);
        visible_end = idx + 1;
    }

    Some(visible_start.map_or(0..0, |start| start..visible_end))
}

#[cfg(test)]
mod tests {
    use super::{contiguous_visible_stage_range, post_select_display_stage_count};

    #[test]
    fn visible_stage_range_borrows_common_course_shapes() {
        assert_eq!(contiguous_visible_stage_range(4, &[0, 1, 2]), Some(3..4));
        assert_eq!(contiguous_visible_stage_range(4, &[2, 3]), Some(0..2));
        assert_eq!(contiguous_visible_stage_range(4, &[0, 1, 2, 3]), Some(0..0));
    }

    #[test]
    fn visible_stage_range_rejects_disjoint_results() {
        assert_eq!(contiguous_visible_stage_range(4, &[1]), None);
        assert_eq!(contiguous_visible_stage_range(5, &[0, 2, 4]), None);
    }

    #[test]
    fn display_stage_count_does_not_materialize_disjoint_results() {
        assert_eq!(post_select_display_stage_count(5, &[0, 2, 4], false), 2);
        assert_eq!(post_select_display_stage_count(5, &[0, 2, 4], true), 5);
        assert_eq!(post_select_display_stage_count(3, &[1, 1, 8], false), 2);
    }
}
