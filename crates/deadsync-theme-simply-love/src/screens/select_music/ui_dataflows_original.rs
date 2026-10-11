// Frozen functions from the starting main commit.
use super::*;

pub(super) fn build_top_grades_grouped_entries(
    ranking: &score_data::SongRankingIndex<'_>,
    workspace: &mut score_data::SongRankingWorkspace,
    chart_type: &str,
    history: &SelectMusicHistoryView,
) -> Vec<MusicWheelEntry> {
    ranking.rank_top_grades(
        chart_type,
        |chart_hash, out| {
            for side_history in &history.sides {
                if let Some(score) = history_score(side_history, chart_hash) {
                    out.push(score);
                }
            }
        },
        workspace,
        song_title_cmp,
    );

    let mut entries: Vec<MusicWheelEntry> =
        Vec::with_capacity(workspace.top_grades().len().saturating_add(20));
    let mut current_group: Option<String> = None;
    let mut current_header_index: Option<usize> = None;
    let mut current_count = 0usize;
    let mut header_idx = 0usize;
    let songs = ranking.songs();

    for (song_ix, best) in workspace.drain_top_grades() {
        let group_name = match best {
            Some(g) => score_data::grade_group_name(g).to_string(),
            None => tr("SelectMusic", "Unplayed").to_string(),
        };
        if current_group.as_deref() != Some(group_name.as_str()) {
            write_header_song_count(&mut entries, current_header_index, current_count);
            entries.push(MusicWheelEntry::PackHeader {
                name: Arc::from(group_name.as_str()),
                original_index: header_idx,
                banner_path: None,
                song_count: 0,
                pack_key: None,
                parent_series: None,
            });
            current_header_index = Some(entries.len() - 1);
            current_group = Some(group_name.clone());
            current_count = 0;
            header_idx += 1;
        }
        current_count += 1;
        entries.push(MusicWheelEntry::Song(Arc::clone(&songs[song_ix])));
    }

    write_header_song_count(&mut entries, current_header_index, current_count);
    entries
}

pub(super) fn build_top_grades_grouped_entries_for_side(
    ranking: &score_data::SongRankingIndex<'_>,
    workspace: &mut score_data::SongRankingWorkspace,
    chart_type: &str,
    history: &SelectMusicHistorySideView,
) -> Vec<MusicWheelEntry> {
    ranking.rank_top_grades(
        chart_type,
        |chart_hash, out| {
            if let Some(score) = history_score(history, chart_hash) {
                out.push(score);
            }
        },
        workspace,
        song_title_cmp,
    );

    let mut entries: Vec<MusicWheelEntry> =
        Vec::with_capacity(workspace.top_grades().len().saturating_add(20));
    let mut current_group: Option<String> = None;
    let mut current_header_index: Option<usize> = None;
    let mut current_count = 0usize;
    let mut header_idx = 0usize;
    let songs = ranking.songs();

    for (song_ix, best) in workspace.drain_top_grades() {
        let group_name = match best {
            Some(g) => score_data::grade_group_name(g).to_string(),
            None => tr("SelectMusic", "Unplayed").to_string(),
        };
        if current_group.as_deref() != Some(group_name.as_str()) {
            write_header_song_count(&mut entries, current_header_index, current_count);
            entries.push(MusicWheelEntry::PackHeader {
                name: Arc::from(group_name.as_str()),
                original_index: header_idx,
                banner_path: None,
                song_count: 0,
                pack_key: None,
                parent_series: None,
            });
            current_header_index = Some(entries.len() - 1);
            current_group = Some(group_name.clone());
            current_count = 0;
            header_idx += 1;
        }
        current_count += 1;
        entries.push(MusicWheelEntry::Song(Arc::clone(&songs[song_ix])));
    }

    write_header_song_count(&mut entries, current_header_index, current_count);
    entries
}

pub(super) fn select_music_lobby_status_text(state: &State) -> Option<String> {
    if let Some(text) = state.lobby_notice_text.clone() {
        return Some(text);
    }
    let mut text = select_music_lobby_lock_text(state)?;
    let prompt = if let Some(elapsed) = lobby_disconnect_hold_elapsed(state) {
        let remaining = (state.lobby_view.disconnect_hold_seconds - elapsed).ceil() as i32;
        let remaining = remaining.max(0);
        tr_fmt(
            "Lobby",
            "DisconnectHoldingFormat",
            &[
                ("remaining", &remaining.to_string()),
                ("s", if remaining == 1 { "" } else { "s" }),
            ],
        )
        .to_string()
    } else {
        tr("Lobby", "DisconnectBasicPrompt").to_string()
    };
    text.push('\n');
    text.push_str(prompt.as_str());
    Some(text)
}
