// Frozen from 58e888a2404a7e7b68e7028b8da444c2784b1032 for differential tests and paired benchmarks.
// Function bodies are unchanged; visibility is widened only where tests need it.
use super::*;

pub(crate) fn entries_with_local_self_state<'a>(
    view: &ScoreboxSideView,
    pane: &'a score_data::LeaderboardPane,
) -> Cow<'a, [score_data::LeaderboardEntry]> {
    let kind = score_data::scorebox_pane_kind(pane);
    let local_self = local_self_score_10000(view, kind);

    if let Some(index) = pane.entries.iter().position(|entry| entry.is_self) {
        let entry = &pane.entries[index];
        if let Some((local_score_10000, local_is_fail)) = local_self
            && local_is_fail
            && score_data::same_score_10000(entry.score, local_score_10000)
        {
            let mut entries = pane.entries.clone();
            let entry = &mut entries[index];
            entry.is_fail = true;
            if entry.machine_tag.is_none() {
                entry.machine_tag = local_self_machine_tag(view);
            }
            return Cow::Owned(entries);
        }
        return Cow::Borrowed(pane.entries.as_slice());
    }

    if let Some(index) = pane
        .entries
        .iter()
        .position(|entry| leaderboard_entry_matches_local_self(view, entry))
    {
        let mut entries = pane.entries.clone();
        let entry = &mut entries[index];
        entry.is_self = true;
        if entry.machine_tag.is_none() {
            entry.machine_tag = local_self_machine_tag(view);
        }
        if let Some((local_score_10000, local_is_fail)) = local_self
            && local_is_fail
            && score_data::same_score_10000(entry.score, local_score_10000)
        {
            entry.is_fail = true;
        }
        return Cow::Owned(entries);
    }

    Cow::Borrowed(pane.entries.as_slice())
}
