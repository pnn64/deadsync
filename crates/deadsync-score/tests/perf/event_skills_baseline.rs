// Frozen from 8a2ecf968 / 0.5.1703.
use super::*;
fn itl_progress_from_submit(
    input: &SubmitEventProgressInput,
    leaderboard: Vec<LeaderboardEntry>,
) -> Option<ItlEventProgress> {
    let itl = input.itl.as_ref()?;
    let score_hundredths = input.itl_score_hundredths?;
    let (clear_type_before, clear_type_after) = event_clear_type_change(itl.progress.as_ref());
    let mut progress = ItlEventProgress {
        kind: EventProgressKind::Itl,
        name: event_name_or_unknown(itl.name.as_str()).to_string(),
        is_doubles: itl.is_doubles,
        score_hundredths,
        score_delta_hundredths: itl.score_delta,
        rate_hundredths: None,
        rate_delta_hundredths: None,
        current_points: itl.top_score_points,
        point_delta: delta_i32(itl.top_score_points, itl.prev_top_score_points),
        current_ranking_points: itl.current_ranking_point_total,
        ranking_delta: delta_i32(
            itl.current_ranking_point_total,
            itl.previous_ranking_point_total,
        ),
        current_song_points: itl.current_song_point_total,
        song_delta: delta_i32(itl.current_song_point_total, itl.previous_song_point_total),
        current_ex_points: itl.current_ex_point_total,
        ex_delta: delta_i32(itl.current_ex_point_total, itl.previous_ex_point_total),
        current_total_points: itl.current_point_total,
        total_delta: delta_i32(itl.current_point_total, itl.previous_point_total),
        total_passes: itl.total_passes,
        clear_type_before,
        clear_type_after,
        stat_improvements: event_stat_improvements(itl.progress.as_ref()),
        skill_improvements: Vec::new(),
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages =
        event_progress_overlay_pages_owned(&progress, itl.progress.as_ref(), leaderboard);
    Some(progress)
}

fn srpg_progress_from_submit(
    input: &SubmitEventProgressInput,
    leaderboard: Vec<LeaderboardEntry>,
) -> Option<ItlEventProgress> {
    let srpg = input.srpg.as_ref()?;
    let score_delta = if input.result.eq_ignore_ascii_case("score-added") {
        input.score_10000 as i32
    } else {
        srpg.score_delta
    };
    let rate_delta = if input.result.eq_ignore_ascii_case("score-added") {
        input.rate_hundredths as i32
    } else {
        srpg.rate_delta
    };
    let mut progress = ItlEventProgress {
        kind: EventProgressKind::Srpg,
        name: event_name_or_unknown(srpg.name.as_str()).to_string(),
        is_doubles: srpg.is_doubles,
        score_hundredths: input.score_10000,
        score_delta_hundredths: score_delta,
        rate_hundredths: Some(input.rate_hundredths),
        rate_delta_hundredths: Some(rate_delta),
        current_points: srpg.top_score_points,
        point_delta: delta_i32(srpg.top_score_points, srpg.prev_top_score_points),
        current_ranking_points: srpg.current_ranking_point_total,
        ranking_delta: delta_i32(
            srpg.current_ranking_point_total,
            srpg.previous_ranking_point_total,
        ),
        current_song_points: srpg.current_song_point_total,
        song_delta: delta_i32(
            srpg.current_song_point_total,
            srpg.previous_song_point_total,
        ),
        current_ex_points: srpg.current_ex_point_total,
        ex_delta: delta_i32(srpg.current_ex_point_total, srpg.previous_ex_point_total),
        current_total_points: srpg.current_point_total,
        total_delta: delta_i32(srpg.current_point_total, srpg.previous_point_total),
        total_passes: srpg.total_passes,
        clear_type_before: None,
        clear_type_after: None,
        stat_improvements: event_stat_improvements(srpg.progress.as_ref()),
        skill_improvements: srpg
            .progress
            .as_ref()
            .map(|progress| progress.skill_improvements.clone())
            .unwrap_or_default(),
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages =
        event_progress_overlay_pages_owned(&progress, srpg.progress.as_ref(), leaderboard);
    Some(progress)
}

pub(super) fn event_progress_from_submit(
    input: &SubmitEventProgressInput,
) -> Vec<ItlEventProgress> {
    let srpg = input
        .srpg
        .as_ref()
        .map(|event| event.leaderboard.clone())
        .unwrap_or_default();
    let itl = input
        .itl
        .as_ref()
        .filter(|_| input.itl_score_hundredths.is_some())
        .map(|event| event.leaderboard.clone())
        .unwrap_or_default();
    event_progress_with_leaderboards(input, srpg, itl)
}

pub(super) fn event_progress_from_submit_owned(
    mut input: SubmitEventProgressInput,
) -> Vec<ItlEventProgress> {
    let srpg = input
        .srpg
        .as_mut()
        .map(|event| std::mem::take(&mut event.leaderboard))
        .unwrap_or_default();
    let itl = input
        .itl
        .as_mut()
        .map(|event| std::mem::take(&mut event.leaderboard))
        .unwrap_or_default();
    event_progress_with_leaderboards(&input, srpg, itl)
}

fn event_progress_with_leaderboards(
    input: &SubmitEventProgressInput,
    srpg: Vec<LeaderboardEntry>,
    itl: Vec<LeaderboardEntry>,
) -> Vec<ItlEventProgress> {
    let count = usize::from(input.srpg.is_some())
        + usize::from(input.itl.is_some() && input.itl_score_hundredths.is_some());
    let mut progress = Vec::with_capacity(count);
    if let Some(srpg) = srpg_progress_from_submit(input, srpg) {
        progress.push(srpg);
    }
    if let Some(itl) = itl_progress_from_submit(input, itl) {
        progress.push(itl);
    }
    progress
}
