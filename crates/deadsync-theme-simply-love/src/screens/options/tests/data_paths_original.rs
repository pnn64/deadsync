// Frozen from b4d4a8b49c21b96b3be66cf05cdfda37b74f9b2f; only names changed.
pub(super) fn original_refresh_score_import_pack_options(state: &mut State) {
    state.score_import_pack_options = score_import_pack_options(state);
    let valid_groups: HashSet<String> = state
        .score_import_pack_options
        .iter()
        .map(|opt| opt.group_key.to_string())
        .collect();
    state
        .score_import_pack_selected
        .retain(|group| valid_groups.contains(group));
    if score_import_selected_pack_count(state) >= state.score_import_pack_options.len() {
        state.score_import_pack_selected.clear();
    }
    let max_idx = state.score_import_pack_options.len().saturating_sub(1);
    if let Some(picker) = state.score_import_pack_picker.as_mut() {
        picker.cursor = picker.cursor.min(max_idx);
    }
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .choice_indices
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = 0;
    }
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .cursor_indices
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = 0;
    }
    sync_pack_picker_summary(state);
}
// Frozen from b4d4a8b49c21b96b3be66cf05cdfda37b74f9b2f; only names changed.
pub(super) fn original_sync_i18n_cache(state: &mut State) {
    let rev = crate::i18n::revision();
    if state.i18n_revision == rev {
        return;
    }
    state.i18n_revision = rev;
    state.display_mode_choices = build_display_mode_choices(&state.monitor_specs);
    state.software_thread_labels = software_thread_choice_labels(&state.software_thread_choices);
    state.sound_device_options = build_sound_device_options(&state.audio_options);
    state.score_import_pack_options = score_import_pack_options(state);
    let new_groups_lc: HashSet<String> = state
        .score_import_pack_options
        .iter()
        .map(|opt| opt.group_key.to_string())
        .collect();
    state
        .score_import_pack_selected
        .retain(|key| new_groups_lc.contains(key));
    sync_pack_picker_summary(state);
    let (sp_packs, sp_filters) = sync_pack_options(state);
    state.sync_pack_choices = sp_packs;
    state.sync_pack_filters = sp_filters;
    #[cfg(target_os = "linux")]
    {
        state.linux_backend_choices = build_linux_backend_choices(&state.audio_options);
    }
    clear_render_cache(state);
}

fn score_import_selected_pack_count(state: &State) -> usize {
    if state.score_import_pack_selected.is_empty() {
        return state.score_import_pack_options.len();
    }
    state
        .score_import_pack_options
        .iter()
        .filter(|opt| state.score_import_pack_selected.contains(&*opt.group_key))
        .count()
}
