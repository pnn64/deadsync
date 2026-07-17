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
    let pack_idx = state.sub[SubmenuKind::SyncPacks]
        .choice_indices
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

pub(super) fn navigation_policy(state: &State) -> shared_pack_sync::NavigationPolicy {
    let choices = &state.sub[SubmenuKind::InputBackend].choice_indices;
    shared_pack_sync::NavigationPolicy {
        only_dedicated_menu_buttons: get_choice_by_id(
            choices,
            INPUT_BACKEND_OPTIONS_ROWS,
            SubRowId::MenuButtons,
        ) == Some(1),
        three_key_navigation: get_choice_by_id(
            choices,
            INPUT_BACKEND_OPTIONS_ROWS,
            SubRowId::MenuNavigation,
        ) == Some(1),
    }
}

#[inline(always)]
pub(super) fn dedicated_three_key_nav(state: &State) -> bool {
    let policy = navigation_policy(state);
    policy.three_key_navigation && policy.only_dedicated_menu_buttons
}

pub(super) fn confidence_percent(state: &State) -> u8 {
    get_choice_by_id(
        &state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::SyncConfidence,
    )
    .map(sync_confidence_from_choice)
    .unwrap_or(80)
}

pub(super) fn begin_pack_sync(state: &mut State, selection: SyncPackSelection) {
    if !matches!(
        state.pack_sync_overlay,
        shared_pack_sync::OverlayState::Hidden
    ) {
        return;
    }

    clear_navigation_holds(state);

    let target_chart_type = state.pack_sync.target_chart_type.as_str();
    let preferred_difficulty_index = state.pack_sync.preferred_difficulty_index;
    let pack_group = selection.pack_group.as_deref();
    let mut targets = Vec::new();

    for pack in &state.song_packs {
        if pack_group.is_some() && Some(pack.group_name.as_str()) != pack_group {
            continue;
        }
        for song in &pack.songs {
            let Some(steps_index) =
                song.best_steps_index(target_chart_type, preferred_difficulty_index)
            else {
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
                song: song.clone(),
                simfile_path: song.simfile_path.clone(),
                song_title: song.display_full_title(false),
                chart_label: shared_pack_sync::chart_label(chart),
                chart_ix,
            });
        }
    }
    let confidence_percent = confidence_percent(state);
    let Some(request) = shared_pack_sync::begin(
        &mut state.pack_sync_overlay,
        crate::SimplyLoveSyncOwner::OptionsPack,
        selection.pack_label.clone(),
        targets,
        confidence_percent,
    ) else {
        log::warn!(
            "Failed to start pack sync for {:?}: no matching charts were found.",
            selection.pack_group
        );
        return;
    };
    queue_sync(state, request);
}

pub(super) fn handle_pack_sync_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    let navigation = navigation_policy(state);
    shared_pack_sync::handle_input(&mut state.pack_sync_overlay, ev, navigation)
}

pub(super) fn begin_pack_sync_from_confirm(state: &mut State) {
    let Some(confirm) = state.sync_pack_confirm.take() else {
        return;
    };
    begin_pack_sync(state, confirm.selection);
}
