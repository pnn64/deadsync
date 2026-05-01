use super::*;

#[derive(Clone, Debug)]
pub(super) struct SyncPackSelection {
    pub(super) pack_group: Option<String>,
    pub(super) pack_label: String,
}

#[derive(Clone, Debug)]
pub(super) struct SyncPackConfirmState {
    pub(super) selection: SyncPackSelection,
    pub(super) active_choice: u8, // 0 = Yes, 1 = No
}

pub(super) fn selected_sync_pack_selection(state: &State) -> SyncPackSelection {
    let pack_idx = state
        .sub[SubmenuKind::SyncPacks].choice_indices
        .get(SYNC_PACK_ROW_PACK_INDEX)
        .copied()
        .unwrap_or(0)
        .min(state.sync_pack_filters.len().saturating_sub(1));
    let pack_group = state.sync_pack_filters.get(pack_idx).cloned().flatten();
    let pack_label = state
        .sync_pack_choices
        .get(pack_idx)
        .cloned()
        .unwrap_or_else(|| tr("OptionsSyncPack", "AllPacks").to_string());
    SyncPackSelection {
        pack_group,
        pack_label,
    }
}

pub(super) fn sync_pack_preferred_difficulty_index() -> usize {
    let profile_data = profile::get();
    let play_style = profile::get_session_play_style();
    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
    if max_diff_index == 0 {
        0
    } else {
        profile_data
            .last_played(play_style)
            .difficulty_index
            .min(max_diff_index)
    }
}

pub(super) fn begin_pack_sync(state: &mut State, selection: SyncPackSelection) {
    if !matches!(
        state.pack_sync_overlay,
        shared_pack_sync::OverlayState::Hidden
    ) {
        return;
    }

    clear_navigation_holds(state);

    let target_chart_type = profile::get_session_play_style().chart_type();
    let preferred_difficulty_index = sync_pack_preferred_difficulty_index();
    let pack_group = selection.pack_group.as_deref();
    let song_cache = crate::game::song::get_song_cache();
    let mut targets = Vec::new();

    for pack in song_cache.iter() {
        if pack_group.is_some() && Some(pack.group_name.as_str()) != pack_group {
            continue;
        }
        for song in &pack.songs {
            let Some(steps_index) = select_music::best_steps_index(
                song.as_ref(),
                target_chart_type,
                preferred_difficulty_index,
            ) else {
                continue;
            };
            let Some(chart_ix) = select_music::selected_chart_ix_for_sync(
                song.as_ref(),
                target_chart_type,
                steps_index,
            ) else {
                continue;
            };
            let Some(chart) = song.charts.get(chart_ix) else {
                continue;
            };
            targets.push(shared_pack_sync::TargetSpec {
                simfile_path: song.simfile_path.clone(),
                song_title: song.display_full_title(false),
                chart_label: shared_pack_sync::chart_label(chart),
                chart_ix,
            });
        }
    }
    drop(song_cache);

    if !shared_pack_sync::begin(
        &mut state.pack_sync_overlay,
        selection.pack_label.clone(),
        targets,
    ) {
        log::warn!(
            "Failed to start pack sync for {:?}: no matching charts were found.",
            selection.pack_group
        );
    }
}

pub(super) fn begin_pack_sync_from_confirm(state: &mut State) {
    let Some(confirm) = state.sync_pack_confirm.take() else {
        return;
    };
    begin_pack_sync(state, confirm.selection);
}
