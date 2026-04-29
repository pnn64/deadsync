use super::*;
use crate::engine::audio;
use crate::game::profile::{
    self as gp, AccelEffectsMask, AppearanceEffectsMask, ErrorBarMask, HoldsMask, InsertMask,
    PlayerSide, RemoveMask, VisualEffectsMask,
};

// ============================ Dispatchers ============================
// Dispatch reads `row.behavior` to decide how to apply input.

/// Returns `(should_persist, persist_side)` for the given player index.
///
/// `pub(super)` so `CustomBinding` arms in `panes/main.rs` and
/// `panes/advanced.rs` can drive their own apply + persist sequence inline.
/// The typed bindings (`NumericBinding`, `ChoiceBinding<T>`) wrap this
/// internally via their `apply_for_player` methods below, so the
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

/// Advance `selected_choice_index[player_idx]` by `delta`, wrapping. Returns
/// the new index, or `None` if the row doesn't exist or has no choices.
pub(super) fn cycle_choice_index(
    state: &mut State,
    player_idx: usize,
    row_id: RowId,
    delta: isize,
    wrap: NavWrap,
) -> Option<usize> {
    let row = state.pane_mut().row_map.get_mut(row_id)?;
    let n = row.choices.len();
    if n == 0 {
        return None;
    }
    let cur = row.selected_choice_index[player_idx] as isize;
    let raw = cur + delta;
    let new_index = match wrap {
        NavWrap::Wrap => raw.rem_euclid(n as isize) as usize,
        NavWrap::Clamp => raw.clamp(0, (n as isize) - 1) as usize,
    };
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

    let outcome = match behavior {
        RowBehavior::Numeric(b) => apply_numeric(state, player_idx, id, delta, b, wrap),
        RowBehavior::Cycle(b) => apply_cycle(state, player_idx, id, delta, &b, wrap),
        RowBehavior::Custom(b) => (b.apply)(state, player_idx, id, delta, wrap),
        RowBehavior::Bitmask(_) => Outcome::NONE,
        RowBehavior::Exit => Outcome::NONE,
    };

    if outcome.persisted && mirror_across_players {
        if let Some(row) = state.pane_mut().row_map.get_mut(id) {
            let v = row.selected_choice_index[player_idx];
            for slot in 0..PLAYER_SLOTS {
                row.selected_choice_index[slot] = v;
            }
        }
    }

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
    wrap: NavWrap,
) -> Outcome {
    let new_index = match cycle_choice_index(state, player_idx, id, delta, wrap) {
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
    wrap: NavWrap,
) -> Outcome {
    let new_index = match cycle_choice_index(state, player_idx, id, delta, wrap) {
        Some(i) => i,
        None => return Outcome::NONE,
    };
    match binding {
        CycleBinding::Bool(b) => b.apply_for_player(state, player_idx, new_index != 0),
        CycleBinding::Index(i) => i.apply_for_player(state, player_idx, new_index),
    }
}

// ========================= Original choice.rs ==========================

pub(super) fn change_choice_for_player(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
    wrap: NavWrap,
) {
    dispatch_behavior_delta(state, asset_manager, player_idx, delta, wrap);
}

pub fn apply_choice_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
    wrap: NavWrap,
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
            change_choice_for_player(state, asset_manager, idx, delta, wrap);
            return;
        }
        if move_inline_focus(state, asset_manager, idx, delta, wrap) {
            audio::play_sfx("assets/sounds/change_value.ogg");
        }
        return;
    }
    change_choice_for_player(state, asset_manager, player_idx, delta, wrap);
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
        ScrollMask::from_bits_truncate(1u8 << (choice_index as u8))
    } else {
        ScrollMask::empty()
    };
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].scroll.toggle(bit);

    // Rebuild the ScrollOption bitmask from the active choices.
    use crate::game::profile::ScrollOption;
    let mut setting = ScrollOption::Normal;
    let mask = state.option_masks[idx].scroll;
    if mask.contains(ScrollMask::REVERSE) {
        setting = setting.union(ScrollOption::Reverse);
    }
    if mask.contains(ScrollMask::SPLIT) {
        setting = setting.union(ScrollOption::Split);
    }
    if mask.contains(ScrollMask::ALTERNATE) {
        setting = setting.union(ScrollOption::Alternate);
    }
    if mask.contains(ScrollMask::CROSS) {
        setting = setting.union(ScrollOption::Cross);
    }
    if mask.contains(ScrollMask::CENTERED) {
        setting = setting.union(ScrollOption::Centered);
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
        HideMask::from_bits_truncate(1u8 << (choice_index as u8))
    } else {
        HideMask::empty()
    };
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].hide.toggle(bit);

    let mask = state.option_masks[idx].hide;
    let hide_targets = mask.contains(HideMask::TARGETS);
    let hide_song_bg = mask.contains(HideMask::BACKGROUND);
    let hide_combo = mask.contains(HideMask::COMBO);
    let hide_lifebar = mask.contains(HideMask::LIFE);
    let hide_score = mask.contains(HideMask::SCORE);
    let hide_danger = mask.contains(HideMask::DANGER);
    let hide_combo_explosions = mask.contains(HideMask::COMBO_EXPLOSIONS);

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

    let mut bits = state.option_masks[idx].insert.bits();
    if (bits & bit) != 0 {
        bits &= !bit;
    } else {
        bits |= bit;
    }
    let mask = InsertMask::from_bits_truncate(bits);
    state.option_masks[idx].insert = mask;
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

    let mut bits = state.option_masks[idx].remove.bits();
    if (bits & bit) != 0 {
        bits &= !bit;
    } else {
        bits |= bit;
    }
    let mask = RemoveMask::from_bits_truncate(bits);
    state.option_masks[idx].remove = mask;
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

    let mut bits = state.option_masks[idx].holds.bits();
    if (bits & bit) != 0 {
        bits &= !bit;
    } else {
        bits |= bit;
    }
    let mask = HoldsMask::from_bits_truncate(bits);
    state.option_masks[idx].holds = mask;
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

    let mut bits = state.option_masks[idx].accel_effects.bits();
    if (bits & bit) != 0 {
        bits &= !bit;
    } else {
        bits |= bit;
    }
    let mask = AccelEffectsMask::from_bits_truncate(bits);
    state.option_masks[idx].accel_effects = mask;
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

    let mut bits = state.option_masks[idx].visual_effects.bits();
    if (bits & bit) != 0 {
        bits &= !bit;
    } else {
        bits |= bit;
    }
    let mask = VisualEffectsMask::from_bits_truncate(bits);
    state.option_masks[idx].visual_effects = mask;
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

    let mut bits = state.option_masks[idx].appearance_effects.bits();
    if (bits & bit) != 0 {
        bits &= !bit;
    } else {
        bits |= bit;
    }
    let mask = AppearanceEffectsMask::from_bits_truncate(bits);
    state.option_masks[idx].appearance_effects = mask;
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
        LifeBarOptionsMask::from_bits_truncate(1u8 << (choice_index as u8))
    } else {
        LifeBarOptionsMask::empty()
    };
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].life_bar_options.toggle(bit);

    let mask = state.option_masks[idx].life_bar_options;
    let rainbow_max = mask.contains(LifeBarOptionsMask::RAINBOW_MAX);
    let responsive_colors = mask.contains(LifeBarOptionsMask::RESPONSIVE_COLORS);
    let show_life_percent = mask.contains(LifeBarOptionsMask::SHOW_LIFE_PERCENT);
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
        FaPlusMask::from_bits_truncate(1u8 << (choice_index as u8))
    } else {
        FaPlusMask::empty()
    };
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].fa_plus.toggle(bit);

    let mask = state.option_masks[idx].fa_plus;
    let window_enabled = mask.contains(FaPlusMask::WINDOW);
    let ex_enabled = mask.contains(FaPlusMask::EX_SCORE);
    let hard_ex_enabled = mask.contains(FaPlusMask::HARD_EX_SCORE);
    let pane_enabled = mask.contains(FaPlusMask::PANE);
    let ten_ms_enabled = mask.contains(FaPlusMask::BLUE_WINDOW_10MS);
    let split_15_10ms_enabled = mask.contains(FaPlusMask::SPLIT_15_10MS);
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
    let bit = match choice_index {
        0 => ResultsExtrasMask::TRACK_EARLY_JUDGMENTS,
        1 => ResultsExtrasMask::SCALE_SCATTERPLOT,
        _ => return,
    };

    state.option_masks[idx].results_extras.toggle(bit);

    let track_early_judgments = state.option_masks[idx]
        .results_extras
        .contains(ResultsExtrasMask::TRACK_EARLY_JUDGMENTS);
    let scale_scatterplot = state.option_masks[idx]
        .results_extras
        .contains(ResultsExtrasMask::SCALE_SCATTERPLOT);
    state.player_profiles[idx].track_early_judgments = track_early_judgments;
    state.player_profiles[idx].scale_scatterplot = scale_scatterplot;

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
        crate::game::profile::update_scale_scatterplot_for_side(side, scale_scatterplot);
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

    let mut bits = state.option_masks[idx].error_bar.bits();
    if (bits & bit) != 0 {
        bits &= !bit;
    } else {
        bits |= bit;
    }
    let mask = ErrorBarMask::from_bits_truncate(bits);
    state.option_masks[idx].error_bar = mask;
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
        ErrorBarOptionsMask::from_bits_truncate(1u8 << (choice_index as u8))
    } else {
        ErrorBarOptionsMask::empty()
    };
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].error_bar_options.toggle(bit);

    let mask = state.option_masks[idx].error_bar_options;
    let up = mask.contains(ErrorBarOptionsMask::MOVE_UP);
    let multi_tick = mask.contains(ErrorBarOptionsMask::MULTI_TICK);
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
        MeasureCounterOptionsMask::from_bits_truncate(1u8 << (choice_index as u8))
    } else {
        MeasureCounterOptionsMask::empty()
    };
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].measure_counter_options.toggle(bit);

    let mask = state.option_masks[idx].measure_counter_options;
    let left = mask.contains(MeasureCounterOptionsMask::MOVE_LEFT);
    let up = mask.contains(MeasureCounterOptionsMask::MOVE_UP);
    let vert = mask.contains(MeasureCounterOptionsMask::VERTICAL_LOOKAHEAD);
    let broken_run = mask.contains(MeasureCounterOptionsMask::BROKEN_RUN_TOTAL);
    let run_timer = mask.contains(MeasureCounterOptionsMask::RUN_TIMER);

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
        EarlyDwMask::from_bits_truncate(1u8 << (choice_index as u8))
    } else {
        EarlyDwMask::empty()
    };
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].early_dw.toggle(bit);

    let mask = state.option_masks[idx].early_dw;
    let hide_judgments = mask.contains(EarlyDwMask::HIDE_JUDGMENTS);
    let hide_flash = mask.contains(EarlyDwMask::HIDE_FLASH);
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
                GameplayExtrasMask::FLASH_COLUMN_FOR_MISS
            } else if choice_str == ge_density.as_ref() {
                GameplayExtrasMask::DENSITY_GRAPH_AT_TOP
            } else if choice_str == ge_column_cues.as_ref() {
                GameplayExtrasMask::COLUMN_CUES
            } else if choice_str == ge_scorebox.as_ref() {
                GameplayExtrasMask::DISPLAY_SCOREBOX
            } else {
                GameplayExtrasMask::empty()
            }
        })
        .unwrap_or(GameplayExtrasMask::empty());
    if bit.is_empty() {
        return;
    }

    state.option_masks[idx].gameplay_extras.toggle(bit);

    let mask = state.option_masks[idx].gameplay_extras;
    let column_flash_on_miss = mask.contains(GameplayExtrasMask::FLASH_COLUMN_FOR_MISS);
    let nps_graph_at_top = mask.contains(GameplayExtrasMask::DENSITY_GRAPH_AT_TOP);
    let column_cues = mask.contains(GameplayExtrasMask::COLUMN_CUES);
    let display_scorebox = mask.contains(GameplayExtrasMask::DISPLAY_SCOREBOX);
    let subtractive_scoring = state.player_profiles[idx].subtractive_scoring;
    let pacemaker = state.player_profiles[idx].pacemaker;

    state.player_profiles[idx].column_flash_on_miss = column_flash_on_miss;
    state.player_profiles[idx].nps_graph_at_top = nps_graph_at_top;
    state.player_profiles[idx].column_cues = column_cues;
    state.player_profiles[idx].display_scorebox = display_scorebox;
    let mut more = GameplayExtrasMoreMask::empty();
    if column_cues {
        more.insert(GameplayExtrasMoreMask::COLUMN_CUES);
    }
    if display_scorebox {
        more.insert(GameplayExtrasMoreMask::DISPLAY_SCOREBOX);
    }
    state.option_masks[idx].gameplay_extras_more = more;

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
    state.start_input = [PlayerStartInput::default(); PLAYER_SLOTS];
    state.help_anim_time = [0.0; PLAYER_SLOTS];
    let active = session_active_players();
    let allow = state.allow_per_player_global_offsets;
    let option_masks = state.option_masks;
    let p = state.pane_mut();
    p.row_tweens = init_row_tweens(&p.row_map, p.selected_row, active, option_masks, allow);
    state.pane_mut().arcade_row_focus = std::array::from_fn(|player_idx| {
        row_allows_arcade_next_row(state, state.pane().selected_row[player_idx])
    });
}

pub(super) fn switch_to_pane(state: &mut State, pane: OptionsPane) {
    if state.current_pane == pane {
        return;
    }
    audio::play_sfx("assets/sounds/start.ogg");

    state.nav_input = [PlayerNavInput::default(); PLAYER_SLOTS];
    state.start_input = [PlayerStartInput::default(); PLAYER_SLOTS];

    state.pane_transition = match state.pane_transition {
        PaneTransition::FadingOut { t, .. } => PaneTransition::FadingOut { target: pane, t },
        _ => PaneTransition::FadingOut {
            target: pane,
            t: 0.0,
        },
    };
}
