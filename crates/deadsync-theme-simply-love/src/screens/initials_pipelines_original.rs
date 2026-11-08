// Frozen from e001ee23bfe8df93aec868d6294eb6de895883df for paired behavioral and allocation checks.
use super::*;
struct ChartScoreCache {
    chart_hash: String,
    entries: Vec<score_data::LeaderboardEntry>,
    used: Vec<bool>,
}

fn find_chart_score_cache<'a>(
    chart_caches: &'a mut Vec<ChartScoreCache>,
    chart_hash: &str,
    leaderboards: &HashMap<String, Vec<score_data::LeaderboardEntry>>,
) -> &'a mut ChartScoreCache {
    if let Some(idx) = chart_caches
        .iter()
        .position(|cache| cache.chart_hash == chart_hash)
    {
        return &mut chart_caches[idx];
    }

    let entries = leaderboards.get(chart_hash).cloned().unwrap_or_default();
    chart_caches.push(ChartScoreCache {
        chart_hash: chart_hash.to_string(),
        used: vec![false; entries.len()],
        entries,
    });

    let last = chart_caches.len().saturating_sub(1);
    &mut chart_caches[last]
}

pub(super) fn build_side_highscore_lists(
    side: profile_data::PlayerSide,
    initials: &str,
    stages: &[stage_stats::StageSummary],
    leaderboards: &HashMap<String, Vec<score_data::LeaderboardEntry>>,
) -> Vec<Option<StageHighScores>> {
    let mut out = vec![None; stages.len()];
    let mut chart_caches: Vec<ChartScoreCache> = Vec::with_capacity(stages.len());
    let side_idx = profile_data::player_side_index(side);

    for stage_idx in (0..stages.len()).rev() {
        let Some(player_stage) = stages
            .get(stage_idx)
            .and_then(|s| s.players.get(side_idx))
            .and_then(|p| p.as_ref())
        else {
            continue;
        };

        let cache = find_chart_score_cache(
            &mut chart_caches,
            player_stage.chart.short_hash.as_str(),
            leaderboards,
        );
        let highlight = consume_highlight_rank(
            cache.entries.as_slice(),
            cache.used.as_mut_slice(),
            initials,
            stage_score_10000(player_stage.score_percent),
        );

        out[stage_idx] = Some(build_stage_highscores(cache.entries.as_slice(), highlight));
    }

    out
}
