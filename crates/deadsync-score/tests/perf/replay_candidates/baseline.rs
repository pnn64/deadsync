// Frozen from 07ef01b31 (0.5.1158); the entry adapter uses the frozen conversion.
use super::*;
use crate::leaderboard::replay_perf::baseline::machine_replay_entry;

fn compare_local_replay_candidates(
    a: &LocalReplayCandidate<'_>,
    b: &LocalReplayCandidate<'_>,
) -> Ordering {
    b.score_percent
        .partial_cmp(&a.score_percent)
        .unwrap_or(Ordering::Equal)
        .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
        .then_with(|| a.initials.cmp(b.initials))
        .then_with(|| a.ordinal.cmp(&b.ordinal))
}

pub(super) fn local_replay_entries(
    mut candidates: Vec<LocalReplayCandidate<'_>>,
    max_entries: usize,
) -> Vec<MachineReplayEntry> {
    candidates.sort_unstable_by(compare_local_replay_candidates);
    let mut entries = Vec::with_capacity(max_entries.min(candidates.len()));
    for candidate in candidates {
        let Some(full) = read_local_score_entry(&candidate.path) else {
            continue;
        };
        let rank = (entries.len() as u32).saturating_add(1);
        entries.push(machine_replay_entry(
            rank,
            MachineReplayPlay {
                initials: candidate.initials.to_owned(),
                score_percent: full.score_percent,
                played_at_ms: candidate.played_at_ms,
                is_fail: grade_from_code(full.grade_code) == Grade::Failed
                    || full.fail_time.is_some(),
                replay_beat0_time_ns: full.beat0_time_ns,
                replay: full.replay,
            },
        ));
        if entries.len() == max_entries {
            break;
        }
    }
    entries
}

// Original ordering step exposed independently of filesystem reads.
pub(super) fn ordered_replay_candidates(
    mut candidates: Vec<LocalReplayCandidate<'_>>,
    _max_entries: usize,
) -> std::vec::IntoIter<LocalReplayCandidate<'_>> {
    candidates.sort_unstable_by(compare_local_replay_candidates);
    candidates.into_iter()
}
