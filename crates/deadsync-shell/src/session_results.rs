use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::score_compat as scores;
use deadsync_profile::player_side_index;
use deadsync_score::{self as score_data, stage_stats::StageSummary};
use deadsync_theme_simply_love::screens::gameplay;
use deadsync_theme_simply_love::views::{EvaluationContextView, EvaluationInitView, ScoreInfo};
use std::borrow::Cow;
use std::ops::Range;

/// Adapt an already authoritative result to the current theme's evaluation
/// view. Artwork, labels and record highlighting do not feed back into it.
pub(crate) fn evaluation_view(
    gs: &gameplay::State,
    stage: &StageSummary,
    context: EvaluationContextView,
) -> EvaluationInitView {
    let policy = context.policy;
    let mut score_info = std::array::from_fn(|_| None);
    let mut fail_stream_progress = [None; MAX_PLAYERS];
    for (player_idx, score_info_slot) in score_info
        .iter_mut()
        .enumerate()
        .take(gs.num_players().min(MAX_PLAYERS))
    {
        let side = deadsync_profile_gameplay::profile_side_from_gameplay(
            gs.setup.session.runtime_player_side(player_idx),
        );
        let Some(player) = stage.players[player_side_index(side)].as_ref() else {
            continue;
        };
        let prof = &gs.profiles()[player_idx];
        let score_percent = player.score_percent;
        let noteskin = gs.noteskin_assets.noteskin[player_idx].clone();
        fail_stream_progress[player_idx] = player.fail_stream_progress;
        let machine_records =
            scores::get_machine_leaderboard_local(player.chart.short_hash.as_str(), usize::MAX);
        let machine_record_highlight_rank =
            score_data::leaderboard_rank_for_score(machine_records.as_slice(), score_percent);
        let personal_records = scores::get_personal_leaderboard_local_for_side(
            player.chart.short_hash.as_str(),
            side,
            usize::MAX,
        );
        let personal_record_highlight_rank =
            score_data::leaderboard_rank_for_score(personal_records.as_slice(), score_percent);
        let score_valid = player.score_valid;
        // Simply Love's "Disqualified" label is driven by PlayerStageStats:IsDisqualified(),
        // not by our broader local ranking-validity heuristics.
        let disqualified = player.disqualified;
        let local_score_valid = score_valid && !disqualified;
        let outcome = gs.individual_song_outcome(player_idx);
        let failed =
            score_data::gameplay_run_failed(outcome.is_failing, outcome.fail_time.is_some());
        let passed = score_data::gameplay_run_passed(
            outcome.song_completed_naturally,
            outcome.is_failing,
            outcome.life,
            outcome.fail_time.is_some(),
        );
        let chart_hash = gs.charts()[player_idx].short_hash.as_str();
        let lua_submit_allowed = score_data::lua_submit_allowed(gs.song().has_lua, chart_hash);
        let course_life_submit_eligible = gs.course_stage_life_submit_eligible(player_idx);
        let expected_groovestats_submit = policy.enable_groovestats
            && passed
            && player.groovestats.valid
            && course_life_submit_eligible
            && prof.groovestats_is_pad_player
            && (!gs.course_display_is_course_stage()
                || policy.autosubmit_course_scores_individually)
            && !prof.groovestats_api_key.trim().is_empty();
        let expected_arrowcloud_submit = policy.enable_arrowcloud
            && !disqualified
            && (passed || (failed && policy.submit_arrowcloud_fails))
            && lua_submit_allowed
            && (course_life_submit_eligible || (failed && policy.submit_arrowcloud_fails))
            && (!gs.course_display_is_course_stage()
                || policy.autosubmit_course_scores_individually)
            && !prof.arrowcloud_api_key.trim().is_empty();
        let earned_machine_record =
            local_score_valid && machine_record_highlight_rank.is_some_and(|rank| rank <= 10);
        let earned_top2_personal =
            local_score_valid && personal_record_highlight_rank.is_some_and(|rank| rank <= 2);
        let machine_record_highlight_rank = local_score_valid
            .then_some(machine_record_highlight_rank)
            .flatten();
        let personal_record_highlight_rank = local_score_valid
            .then_some(personal_record_highlight_rank)
            .flatten();
        let show_machine_personal_split = !earned_machine_record && earned_top2_personal;

        *score_info_slot = Some(ScoreInfo {
                song: stage.song.clone(),
                chart: player.chart.clone(),
                course_graph_stages: Vec::new(),
                side,
                profile_name: player.profile_name.clone(),
                score_valid: player.score_valid,
                disqualified: player.disqualified,
                expected_groovestats_submit,
                expected_arrowcloud_submit,
                groovestats: player.groovestats.clone(),
                itl: player.itl.clone(),
                judgment_counts: player.judgment_counts,
                score_percent: player.score_percent,
                earned_grade_points: player.earned_grade_points,
                possible_grade_points: player.possible_grade_points,
                grade: player.grade,
                speed_mod: gs.scroll_speed_for_player(player_idx),
                mods_text: deadsync_theme_simply_love::screens::components::gameplay::notefield::preferred_mods_text(
                    gs, player_idx,
                ),
                hands_achieved: player.hands_achieved,
                hands_total: player.hands_total,
                holds_held: player.holds_held,
                holds_held_for_score: player.holds_held_for_score,
                holds_total: player.holds_total,
                rolls_held: player.rolls_held,
                rolls_held_for_score: player.rolls_held_for_score,
                rolls_total: player.rolls_total,
                mines_hit_for_score: player.mines_hit_for_score,
                mines_avoided: player.mines_avoided,
                mines_total: player.mines_total,
                timing: player.timing,
                arrow_timing: player.arrow_timing.clone(),
                scatter: player.scatter.clone(),
                scatter_worst_window_ms: player.scatter_worst_window_ms,
                histogram: player.histogram.clone(),
                graph_first_second: player.graph_first_second,
                graph_last_second: player.graph_last_second,
                music_rate: stage.music_rate,
                life_history: player.life_history.clone(),
                fail_time: player.fail_time,
                window_counts: player.window_counts,
                window_counts_10ms: player.window_counts_10ms,
                ex_score_percent: player.ex_score_percent,
                hard_ex_score_percent: player.hard_ex_score_percent,
                calories_burned: player.calories_burned,
                column_judgments: player.column_judgments.clone(),
                noteskin,
                show_fa_plus_window: prof.show_fa_plus_window,
                show_ex_score: player.show_ex_score,
                show_hard_ex_score: player.show_hard_ex_score,
                show_fa_plus_pane: player.show_fa_plus_pane,
                track_early_judgments: player.track_early_judgments,
                dim_post_fail_scatter: player.dim_post_fail_scatter,
                disabled_timing_windows: prof.timing_windows.disabled_windows(),
                machine_records,
                machine_record_highlight_rank,
                personal_records,
                personal_record_highlight_rank,
                show_machine_personal_split,
            });
    }
    EvaluationInitView {
        score_info,
        fail_stream_progress,
        context,
        stage_duration_seconds: stage.duration_seconds,
    }
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

pub fn fill_stage_indices(
    out: &mut Vec<usize>,
    stage_count: usize,
    hidden_indices: &[usize],
    show_course_individual_scores: bool,
) {
    out.clear();
    out.reserve(stage_count);
    if show_course_individual_scores || hidden_indices.is_empty() {
        out.extend(0..stage_count);
        return;
    }
    if let Some(range) = contiguous_visible_stage_range(stage_count, hidden_indices) {
        out.extend(range);
        return;
    }
    out.extend((0..stage_count).filter(|index| hidden_indices.binary_search(index).is_err()));
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
    use super::{
        contiguous_visible_stage_range, fill_stage_indices, post_select_display_stage_count,
    };

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

    #[test]
    fn display_stage_indices_cover_contiguous_and_disjoint_results() {
        let mut indices = Vec::new();
        fill_stage_indices(&mut indices, 4, &[0, 1, 2], false);
        assert_eq!(indices, [3]);

        fill_stage_indices(&mut indices, 5, &[0, 2, 4], false);
        assert_eq!(indices, [1, 3]);

        fill_stage_indices(&mut indices, 3, &[1], true);
        assert_eq!(indices, [0, 1, 2]);
    }
}
