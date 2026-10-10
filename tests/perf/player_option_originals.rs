use super::*;

pub(crate) fn original_dispatch_behavior_delta(
    state: &mut State,
    player_idx: usize,
    delta: isize,
    wrap: NavWrap,
) {
    if state.pane().row_map.is_empty() {
        return;
    }
    let player_idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index =
        state.pane().selected_row[player_idx].min(state.pane().row_map.len().saturating_sub(1));
    let Some(&id) = state.pane().row_map.display_order().get(row_index) else {
        return;
    };
    let Some((behavior, mirror_across_players)) = state
        .pane()
        .row_map
        .get(id)
        .map(|r| (r.behavior, r.mirror_across_players))
    else {
        return;
    };
    let before = (
        state.player_options[player_idx].clone(),
        state.judgment_palette_ids[player_idx].clone(),
        state.heart_rate_device_ids[player_idx].clone(),
        state.max_heart_rate[player_idx],
    );

    let outcome = match behavior {
        RowBehavior::Numeric(b) => apply_numeric(state, player_idx, id, delta, b, wrap),
        RowBehavior::Cycle(b) => apply_cycle(state, player_idx, id, delta, &b, wrap),
        RowBehavior::Custom(b) => (b.apply)(state, player_idx, id, delta, wrap),
        RowBehavior::Bitmask(_) => Outcome::NONE,
        RowBehavior::Exit => Outcome::NONE,
    };

    if outcome.persisted
        && id == RowId::Perspective
        && state.player_options[player_idx].perspective != before.0.perspective
    {
        mark_speed_header_dirty(state, player_idx);
    }

    if outcome.persisted
        && mirror_across_players
        && let Some(row) = state.pane_mut().row_map.get_mut(id)
    {
        let v = row.selected_choice_index[player_idx];
        for slot in 0..PLAYER_SLOTS {
            row.selected_choice_index[slot] = v;
        }
    }

    if outcome.persisted {
        if (
            state.player_options[player_idx].clone(),
            state.judgment_palette_ids[player_idx].clone(),
            state.heart_rate_device_ids[player_idx].clone(),
            state.max_heart_rate[player_idx],
        ) != before
        {
            queue_profile_update(state, player_idx);
        }
        super::sync_inline_intent_from_row(state, player_idx, row_index);
        queue_sfx(state, CHANGE_VALUE_SFX);
    }
    if outcome.changed_visibility {
        super::sync_selected_rows_with_visibility(state, state.active);
    }
}

pub(crate) fn original_toggle_bitmask_row_generic(
    state: &mut State,
    player_idx: usize,
    id: RowId,
) -> bool {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    let focused_id = match state.pane().row_map.display_order().get(row_index) {
        Some(&fid) => fid,
        None => return false,
    };
    if focused_id != id {
        return false;
    }

    let (init, writeback) = match state.pane().row_map.get(id).map(|r| r.behavior) {
        Some(RowBehavior::Bitmask(BitmaskBinding::Generic { init, writeback })) => {
            (init, writeback)
        }
        _ => return false,
    };
    let before = (
        state.player_options[idx].clone(),
        state.heart_rate_device_ids[idx].clone(),
    );

    let row = state.pane().row_map.row(id);
    let choice_index = row.selected_choice_index[idx];
    let bit = match writeback.bit_mapping.bit_for_choice(choice_index) {
        Some(b) if b != 0 => b,
        _ => return false,
    };

    let cur = (init.get_active)(&state.option_masks[idx]);
    let new_bits = cur ^ bit;
    (init.set_active)(&mut state.option_masks[idx], new_bits);
    let stored = (init.get_active)(&state.option_masks[idx]);

    (writeback.project)(
        &mut state.option_masks[idx],
        &mut state.player_options[idx],
        stored,
    );

    if (
        state.player_options[idx].clone(),
        state.heart_rate_device_ids[idx].clone(),
    ) != before
    {
        queue_profile_update(state, idx);
    }

    if writeback.sync_visibility {
        sync_selected_rows_with_visibility(state, state.active);
    }

    queue_sfx(state, CHANGE_VALUE_SFX);
    true
}
