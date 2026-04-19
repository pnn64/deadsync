use super::*;
use crate::engine::audio;
use crate::game::profile::{self as gp, PlayerSide};

// ============================ Dispatchers ============================
// Dispatch reads `row.behavior` to decide how to apply input.

/// Returns `(should_persist, persist_side)` for the given player index.
///
/// `pub(super)` so `CustomBinding` arms in `panes/main.rs` and
/// `panes/advanced.rs` can drive their own apply + persist sequence inline.
/// The typed bindings (`NumericBinding`, `ChoiceBinding<T>`, `NoteSkinBinding`)
/// wrap this internally via their `apply_for_player` methods below, so the
/// dispatcher itself never reads it.
pub(super) fn persist_ctx(player_idx: usize) -> (bool, PlayerSide) {
    let play_style = gp::get_session_play_style();
    let persisted_idx = super::session_persisted_player_idx();
    let should_persist = play_style == gp::PlayStyle::Versus || player_idx == persisted_idx;
    let side = if player_idx == P1 {
        PlayerSide::P1
    } else {
        PlayerSide::P2
    };
    (should_persist, side)
}

// ========================= Self-contained binding application =========================
// Each typed binding owns the full "write to in-memory profile + conditionally
// persist to the on-disk profile for the right side" dance. The dispatcher
// hands off a freshly-computed value and reads back an `Outcome`; it does not
// need to know about `PlayerSide`, `persist_ctx`, or `persist_for_side`.

impl NumericBinding {
    #[inline]
    pub(super) fn apply_for_player(
        &self,
        state: &mut State,
        player_idx: usize,
        value: i32,
    ) -> Outcome {
        let outcome = (self.apply)(&mut state.player_profiles[player_idx], value);
        let (should_persist, side) = persist_ctx(player_idx);
        if should_persist {
            (self.persist_for_side)(side, value);
        }
        outcome
    }
}

impl<T: Copy + 'static> ChoiceBinding<T> {
    #[inline]
    pub(super) fn apply_for_player(
        &self,
        state: &mut State,
        player_idx: usize,
        value: T,
    ) -> Outcome {
        let outcome = (self.apply)(&mut state.player_profiles[player_idx], value);
        let (should_persist, side) = persist_ctx(player_idx);
        if should_persist {
            (self.persist_for_side)(side, value);
        }
        outcome
    }
}

impl NoteSkinBinding {
    #[inline]
    pub(super) fn apply_for_player(
        &self,
        state: &mut State,
        player_idx: usize,
        choice: &str,
    ) -> Outcome {
        let (should_persist, side) = persist_ctx(player_idx);
        (self.apply)(state, player_idx, choice, should_persist, side);
        Outcome::persisted()
    }
}

/// Advance `selected_choice_index[player_idx]` by `delta`, wrapping. Returns
/// the new index, or `None` if the row doesn't exist or has no choices.
pub(super) fn cycle_choice_index(
    state: &mut State,
    player_idx: usize,
    row_id: RowId,
    delta: isize,
) -> Option<usize> {
    let row = state.pane_mut().row_map.get_mut(row_id)?;
    let n = row.choices.len();
    if n == 0 {
        return None;
    }
    let cur = row.selected_choice_index[player_idx] as isize;
    let new_index = (cur + delta).rem_euclid(n as isize) as usize;
    row.selected_choice_index[player_idx] = new_index;
    Some(new_index)
}

/// L/R input dispatcher. Looks up the focused row's `RowBehavior` and routes the
/// delta. Plays the change-value SFX and syncs visibility based on the
/// returned `Outcome`.
pub(super) fn dispatch_behavior_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
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
    let Some(behavior) = state.pane().row_map.get(id).map(|r| r.behavior) else {
        return;
    };

    let outcome = match behavior {
        RowBehavior::Numeric(b) => apply_numeric(state, player_idx, id, delta, b),
        RowBehavior::Cycle(b) => apply_cycle(state, player_idx, id, delta, &b),
        RowBehavior::Custom(b) => (b.apply)(state, player_idx, id, delta),
        RowBehavior::Bitmask(_) => Outcome::NONE,
        RowBehavior::Action(ActionRow::Exit) => Outcome::NONE,
        RowBehavior::Action(ActionRow::WhatComesNext) => {
            apply_what_comes_next_cycle(state, player_idx, id, delta)
        }
    };

    if outcome.persisted {
        super::sync_inline_intent_from_row(state, asset_manager, player_idx, row_index);
        audio::play_sfx("assets/sounds/change_value.ogg");
    }
    if outcome.changed_visibility {
        super::sync_selected_rows_with_visibility(state, super::session_active_players());
    }
}

/// Start input dispatcher. Only Bitmask rows are handled here — the
/// `toggle_*_row` helpers already play their own SFX and sync visibility.
/// Returns true if the dispatcher handled the row (Bitmask behavior), false
/// otherwise.
pub(super) fn dispatch_behavior_toggle(state: &mut State, player_idx: usize, id: RowId) -> bool {
    let Some(RowBehavior::Bitmask(b)) = state.pane().row_map.get(id).map(|r| r.behavior) else {
        return false;
    };
    (b.toggle)(state, player_idx);
    true
}

fn apply_numeric(
    state: &mut State,
    player_idx: usize,
    id: RowId,
    delta: isize,
    binding: NumericBinding,
) -> Outcome {
    let new_index = match cycle_choice_index(state, player_idx, id, delta) {
        Some(i) => i,
        None => return Outcome::NONE,
    };
    let choice = state
        .pane()
        .row_map
        .get(id)
        .and_then(|r| r.choices.get(new_index))
        .cloned();
    let Some(choice) = choice else {
        return Outcome::NONE;
    };
    let Some(value) = (binding.parse)(&choice) else {
        return Outcome::persisted();
    };
    binding.apply_for_player(state, player_idx, value)
}

fn apply_cycle(
    state: &mut State,
    player_idx: usize,
    id: RowId,
    delta: isize,
    binding: &CycleBinding,
) -> Outcome {
    let new_index = match cycle_choice_index(state, player_idx, id, delta) {
        Some(i) => i,
        None => return Outcome::NONE,
    };
    match binding {
        CycleBinding::Bool(b) => b.apply_for_player(state, player_idx, new_index != 0),
        CycleBinding::Index(i) => i.apply_for_player(state, player_idx, new_index),
        CycleBinding::NoteSkin(n) => {
            let choice = state
                .pane()
                .row_map
                .get(id)
                .and_then(|r| r.choices.get(new_index))
                .cloned()
                .unwrap_or_default();
            n.apply_for_player(state, player_idx, &choice)
        }
    }
}

fn apply_what_comes_next_cycle(
    state: &mut State,
    player_idx: usize,
    id: RowId,
    delta: isize,
) -> Outcome {
    let new_index = match cycle_choice_index(state, player_idx, id, delta) {
        Some(i) => i,
        None => return Outcome::NONE,
    };
    if let Some(row) = state.pane_mut().row_map.get_mut(id) {
        for slot in 0..PLAYER_SLOTS {
            row.selected_choice_index[slot] = new_index;
        }
    }
    Outcome::persisted()
}

// ========================= Original choice.rs ==========================

pub(super) fn change_choice_for_player(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) {
    dispatch_behavior_delta(state, asset_manager, player_idx, delta);
}

pub fn apply_choice_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) {
    if state.pane().row_map.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))
        && row_supports_inline_nav(row)
    {
        if state.current_pane == OptionsPane::Main || row_selects_on_focus_move(row.id) {
            change_choice_for_player(state, asset_manager, idx, delta);
            return;
        }
        if move_inline_focus(state, asset_manager, idx, delta) {
            audio::play_sfx("assets/sounds/change_value.ogg");
        }
        return;
    }
    change_choice_for_player(state, asset_manager, player_idx, delta);
}

pub(super) fn toggle_scroll_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Scroll {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.scroll_active_mask[idx] & bit) != 0 {
        state.scroll_active_mask[idx] &= !bit;
    } else {
        state.scroll_active_mask[idx] |= bit;
    }

    // Rebuild the ScrollOption bitmask from the active choices.
    use crate::game::profile::ScrollOption;
    let mut setting = ScrollOption::Normal;
    if state.scroll_active_mask[idx] != 0 {
        if (state.scroll_active_mask[idx] & (1u8 << 0)) != 0 {
            setting = setting.union(ScrollOption::Reverse);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 1)) != 0 {
            setting = setting.union(ScrollOption::Split);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 2)) != 0 {
            setting = setting.union(ScrollOption::Alternate);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 3)) != 0 {
            setting = setting.union(ScrollOption::Cross);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 4)) != 0 {
            setting = setting.union(ScrollOption::Centered);
        }
    }
    state.player_profiles[idx].scroll_option = setting;
    state.player_profiles[idx].reverse_scroll = setting.contains(ScrollOption::Reverse);
    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_scroll_option_for_side(side, setting);
    }
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_hide_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Hide {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.hide_active_mask[idx] & bit) != 0 {
        state.hide_active_mask[idx] &= !bit;
    } else {
        state.hide_active_mask[idx] |= bit;
    }

    let hide_targets = (state.hide_active_mask[idx] & (1u8 << 0)) != 0;
    let hide_song_bg = (state.hide_active_mask[idx] & (1u8 << 1)) != 0;
    let hide_combo = (state.hide_active_mask[idx] & (1u8 << 2)) != 0;
    let hide_lifebar = (state.hide_active_mask[idx] & (1u8 << 3)) != 0;
    let hide_score = (state.hide_active_mask[idx] & (1u8 << 4)) != 0;
    let hide_danger = (state.hide_active_mask[idx] & (1u8 << 5)) != 0;
    let hide_combo_explosions = (state.hide_active_mask[idx] & (1u8 << 6)) != 0;

    state.player_profiles[idx].hide_targets = hide_targets;
    state.player_profiles[idx].hide_song_bg = hide_song_bg;
    state.player_profiles[idx].hide_combo = hide_combo;
    state.player_profiles[idx].hide_lifebar = hide_lifebar;
    state.player_profiles[idx].hide_score = hide_score;
    state.player_profiles[idx].hide_danger = hide_danger;
    state.player_profiles[idx].hide_combo_explosions = hide_combo_explosions;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_hide_options_for_side(
            side,
            hide_targets,
            hide_song_bg,
            hide_combo,
            hide_lifebar,
            hide_score,
            hide_danger,
            hide_combo_explosions,
        );
    }

    sync_selected_rows_with_visibility(state, session_active_players());
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_insert_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Insert {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 7 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.insert_active_mask[idx] & bit) != 0 {
        state.insert_active_mask[idx] &= !bit;
    } else {
        state.insert_active_mask[idx] |= bit;
    }
    state.insert_active_mask[idx] =
        crate::game::profile::normalize_insert_mask(state.insert_active_mask[idx]);
    let mask = state.insert_active_mask[idx];
    state.player_profiles[idx].insert_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_insert_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_remove_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Remove {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.remove_active_mask[idx] & bit) != 0 {
        state.remove_active_mask[idx] &= !bit;
    } else {
        state.remove_active_mask[idx] |= bit;
    }
    state.remove_active_mask[idx] =
        crate::game::profile::normalize_remove_mask(state.remove_active_mask[idx]);
    let mask = state.remove_active_mask[idx];
    state.player_profiles[idx].remove_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_remove_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_holds_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Holds {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .pane()
            .row_map
            .row(state.pane().row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.holds_active_mask[idx] & bit) != 0 {
        state.holds_active_mask[idx] &= !bit;
    } else {
        state.holds_active_mask[idx] |= bit;
    }
    state.holds_active_mask[idx] =
        crate::game::profile::normalize_holds_mask(state.holds_active_mask[idx]);
    let mask = state.holds_active_mask[idx];
    state.player_profiles[idx].holds_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_holds_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_accel_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Accel {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .pane()
            .row_map
            .row(state.pane().row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.accel_effects_active_mask[idx] & bit) != 0 {
        state.accel_effects_active_mask[idx] &= !bit;
    } else {
        state.accel_effects_active_mask[idx] |= bit;
    }
    state.accel_effects_active_mask[idx] =
        crate::game::profile::normalize_accel_effects_mask(state.accel_effects_active_mask[idx]);
    let mask = state.accel_effects_active_mask[idx];
    state.player_profiles[idx].accel_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_accel_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_visual_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Effect {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 10 {
        1u16 << (choice_index as u16)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.visual_effects_active_mask[idx] & bit) != 0 {
        state.visual_effects_active_mask[idx] &= !bit;
    } else {
        state.visual_effects_active_mask[idx] |= bit;
    }
    state.visual_effects_active_mask[idx] =
        crate::game::profile::normalize_visual_effects_mask(state.visual_effects_active_mask[idx]);
    let mask = state.visual_effects_active_mask[idx];
    state.player_profiles[idx].visual_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_visual_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_appearance_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::Appearance {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .pane()
            .row_map
            .row(state.pane().row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.appearance_effects_active_mask[idx] & bit) != 0 {
        state.appearance_effects_active_mask[idx] &= !bit;
    } else {
        state.appearance_effects_active_mask[idx] |= bit;
    }
    state.appearance_effects_active_mask[idx] =
        crate::game::profile::normalize_appearance_effects_mask(
            state.appearance_effects_active_mask[idx],
        );
    let mask = state.appearance_effects_active_mask[idx];
    state.player_profiles[idx].appearance_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_appearance_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_life_bar_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::LifeBarOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 3 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.life_bar_options_active_mask[idx] & bit) != 0 {
        state.life_bar_options_active_mask[idx] &= !bit;
    } else {
        state.life_bar_options_active_mask[idx] |= bit;
    }

    let rainbow_max = (state.life_bar_options_active_mask[idx] & (1u8 << 0)) != 0;
    let responsive_colors = (state.life_bar_options_active_mask[idx] & (1u8 << 1)) != 0;
    let show_life_percent = (state.life_bar_options_active_mask[idx] & (1u8 << 2)) != 0;
    state.player_profiles[idx].rainbow_max = rainbow_max;
    state.player_profiles[idx].responsive_colors = responsive_colors;
    state.player_profiles[idx].show_life_percent = show_life_percent;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_rainbow_max_for_side(side, rainbow_max);
        crate::game::profile::update_responsive_colors_for_side(side, responsive_colors);
        crate::game::profile::update_show_life_percent_for_side(side, show_life_percent);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_fa_plus_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::FAPlusOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .pane()
            .row_map
            .row(state.pane().row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.fa_plus_active_mask[idx] & bit) != 0 {
        state.fa_plus_active_mask[idx] &= !bit;
    } else {
        state.fa_plus_active_mask[idx] |= bit;
    }

    let window_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 0)) != 0;
    let ex_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 1)) != 0;
    let hard_ex_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 2)) != 0;
    let pane_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 3)) != 0;
    let ten_ms_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 4)) != 0;
    let split_15_10ms_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 5)) != 0;
    state.player_profiles[idx].show_fa_plus_window = window_enabled;
    state.player_profiles[idx].show_ex_score = ex_enabled;
    state.player_profiles[idx].show_hard_ex_score = hard_ex_enabled;
    state.player_profiles[idx].show_fa_plus_pane = pane_enabled;
    state.player_profiles[idx].fa_plus_10ms_blue_window = ten_ms_enabled;
    state.player_profiles[idx].split_15_10ms = split_15_10ms_enabled;
    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_show_fa_plus_window_for_side(side, window_enabled);
        crate::game::profile::update_show_ex_score_for_side(side, ex_enabled);
        crate::game::profile::update_show_hard_ex_score_for_side(side, hard_ex_enabled);
        crate::game::profile::update_show_fa_plus_pane_for_side(side, pane_enabled);
        crate::game::profile::update_fa_plus_10ms_blue_window_for_side(side, ten_ms_enabled);
        crate::game::profile::update_split_15_10ms_for_side(side, split_15_10ms_enabled);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_results_extras_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::ResultsExtras {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 1 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.results_extras_active_mask[idx] & bit) != 0 {
        state.results_extras_active_mask[idx] &= !bit;
    } else {
        state.results_extras_active_mask[idx] |= bit;
    }

    let track_early_judgments = (state.results_extras_active_mask[idx] & (1u8 << 0)) != 0;
    state.player_profiles[idx].track_early_judgments = track_early_judgments;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_track_early_judgments_for_side(side, track_early_judgments);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_error_bar_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::ErrorBar {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 5 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.error_bar_active_mask[idx] & bit) != 0 {
        state.error_bar_active_mask[idx] &= !bit;
    } else {
        state.error_bar_active_mask[idx] |= bit;
    }
    state.error_bar_active_mask[idx] =
        crate::game::profile::normalize_error_bar_mask(state.error_bar_active_mask[idx]);
    let mask = state.error_bar_active_mask[idx];
    state.player_profiles[idx].error_bar_active_mask = mask;
    state.player_profiles[idx].error_bar = crate::game::profile::error_bar_style_from_mask(mask);
    state.player_profiles[idx].error_bar_text =
        crate::game::profile::error_bar_text_from_mask(mask);

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_error_bar_mask_for_side(side, mask);
    }

    sync_selected_rows_with_visibility(state, session_active_players());
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_error_bar_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::ErrorBarOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 2 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.error_bar_options_active_mask[idx] & bit) != 0 {
        state.error_bar_options_active_mask[idx] &= !bit;
    } else {
        state.error_bar_options_active_mask[idx] |= bit;
    }

    let up = (state.error_bar_options_active_mask[idx] & (1u8 << 0)) != 0;
    let multi_tick = (state.error_bar_options_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].error_bar_up = up;
    state.player_profiles[idx].error_bar_multi_tick = multi_tick;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_error_bar_options_for_side(side, up, multi_tick);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_measure_counter_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::MeasureCounterOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 5 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.measure_counter_options_active_mask[idx] & bit) != 0 {
        state.measure_counter_options_active_mask[idx] &= !bit;
    } else {
        state.measure_counter_options_active_mask[idx] |= bit;
    }

    let left = (state.measure_counter_options_active_mask[idx] & (1u8 << 0)) != 0;
    let up = (state.measure_counter_options_active_mask[idx] & (1u8 << 1)) != 0;
    let vert = (state.measure_counter_options_active_mask[idx] & (1u8 << 2)) != 0;
    let broken_run = (state.measure_counter_options_active_mask[idx] & (1u8 << 3)) != 0;
    let run_timer = (state.measure_counter_options_active_mask[idx] & (1u8 << 4)) != 0;

    state.player_profiles[idx].measure_counter_left = left;
    state.player_profiles[idx].measure_counter_up = up;
    state.player_profiles[idx].measure_counter_vert = vert;
    state.player_profiles[idx].broken_run = broken_run;
    state.player_profiles[idx].run_timer = run_timer;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_measure_counter_options_for_side(
            side, left, up, vert, broken_run, run_timer,
        );
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_early_dw_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::EarlyDecentWayOffOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 2 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.early_dw_active_mask[idx] & bit) != 0 {
        state.early_dw_active_mask[idx] &= !bit;
    } else {
        state.early_dw_active_mask[idx] |= bit;
    }

    let hide_judgments = (state.early_dw_active_mask[idx] & (1u8 << 0)) != 0;
    let hide_flash = (state.early_dw_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].hide_early_dw_judgments = hide_judgments;
    state.player_profiles[idx].hide_early_dw_flash = hide_flash;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_early_dw_options_for_side(side, hide_judgments, hide_flash);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_gameplay_extras_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx];
    if let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))
    {
        if row.id != RowId::GameplayExtras {
            return;
        }
    } else {
        return;
    }

    let row = state
        .pane()
        .row_map
        .row(state.pane().row_map.id_at(row_index));
    let choice_index = row.selected_choice_index[idx];
    let ge_flash = tr("PlayerOptions", "GameplayExtrasFlashColumnForMiss");
    let ge_density = tr("PlayerOptions", "GameplayExtrasDensityGraphAtTop");
    let ge_column_cues = tr("PlayerOptions", "GameplayExtrasColumnCues");
    let ge_scorebox = tr("PlayerOptions", "GameplayExtrasDisplayScorebox");
    let bit = row
        .choices
        .get(choice_index)
        .map(|choice| {
            let choice_str = choice.as_str();
            if choice_str == ge_flash.as_ref() {
                1u8 << 0
            } else if choice_str == ge_density.as_ref() {
                1u8 << 1
            } else if choice_str == ge_column_cues.as_ref() {
                1u8 << 2
            } else if choice_str == ge_scorebox.as_ref() {
                1u8 << 3
            } else {
                0
            }
        })
        .unwrap_or(0);
    if bit == 0 {
        return;
    }

    if (state.gameplay_extras_active_mask[idx] & bit) != 0 {
        state.gameplay_extras_active_mask[idx] &= !bit;
    } else {
        state.gameplay_extras_active_mask[idx] |= bit;
    }

    let column_flash_on_miss = (state.gameplay_extras_active_mask[idx] & (1u8 << 0)) != 0;
    let nps_graph_at_top = (state.gameplay_extras_active_mask[idx] & (1u8 << 1)) != 0;
    let column_cues = (state.gameplay_extras_active_mask[idx] & (1u8 << 2)) != 0;
    let display_scorebox = (state.gameplay_extras_active_mask[idx] & (1u8 << 3)) != 0;
    let subtractive_scoring = state.player_profiles[idx].subtractive_scoring;
    let pacemaker = state.player_profiles[idx].pacemaker;

    state.player_profiles[idx].column_flash_on_miss = column_flash_on_miss;
    state.player_profiles[idx].nps_graph_at_top = nps_graph_at_top;
    state.player_profiles[idx].column_cues = column_cues;
    state.player_profiles[idx].display_scorebox = display_scorebox;
    state.gameplay_extras_more_active_mask[idx] =
        (column_cues as u8) | ((display_scorebox as u8) << 1);

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_gameplay_extras_for_side(
            side,
            column_flash_on_miss,
            subtractive_scoring,
            pacemaker,
            nps_graph_at_top,
        );
        crate::game::profile::update_column_cues_for_side(side, column_cues);
        crate::game::profile::update_display_scorebox_for_side(side, display_scorebox);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn apply_pane(state: &mut State, pane: OptionsPane) {
    // Row_maps are pre-built at init() and live in `State::panes`, so a
    // pane switch does not rebuild rows or recompute masks (masks are kept up
    // to date incrementally by toggle handlers). Switching is now a structural
    // operation: change the active pane, reset the destination pane's cursor
    // to the top, and recompute its row tweens for the new layout.
    state.current_pane = pane;
    state.pane_mut().reset_cursor();
    state.start_held_since = [None; PLAYER_SLOTS];
    state.start_last_triggered_at = [None; PLAYER_SLOTS];
    state.help_anim_time = [0.0; PLAYER_SLOTS];
    let active = session_active_players();
    let hide = state.hide_active_mask;
    let error_bar = state.error_bar_active_mask;
    let allow = state.allow_per_player_global_offsets;
    let p = state.pane_mut();
    p.row_tweens = init_row_tweens(&p.row_map, p.selected_row, active, hide, error_bar, allow);
    state.pane_mut().arcade_row_focus = std::array::from_fn(|player_idx| {
        row_allows_arcade_next_row(state, state.pane().selected_row[player_idx])
    });
}

pub(super) fn switch_to_pane(state: &mut State, pane: OptionsPane) {
    if state.current_pane == pane {
        return;
    }
    audio::play_sfx("assets/sounds/start.ogg");

    state.nav_key_held_direction = [None; PLAYER_SLOTS];
    state.nav_key_held_since = [None; PLAYER_SLOTS];
    state.nav_key_last_scrolled_at = [None; PLAYER_SLOTS];
    state.start_held_since = [None; PLAYER_SLOTS];
    state.start_last_triggered_at = [None; PLAYER_SLOTS];

    state.pane_transition = match state.pane_transition {
        PaneTransition::FadingOut { t, .. } => PaneTransition::FadingOut { target: pane, t },
        _ => PaneTransition::FadingOut {
            target: pane,
            t: 0.0,
        },
    };
}
