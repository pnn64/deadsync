use super::*;
use crate::engine::audio;
use crate::game::profile::{self as gp, PlayerSide};

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

/// Start input dispatcher. Only Bitmask rows are handled here.
/// Returns true if the dispatcher handled the row (Bitmask behavior), false
/// otherwise. All bitmask rows route through `toggle_bitmask_row_generic`.
pub(super) fn dispatch_behavior_toggle(state: &mut State, player_idx: usize, id: RowId) -> bool {
    let Some(RowBehavior::Bitmask(_)) = state.pane().row_map.get(id).map(|r| r.behavior) else {
        return false;
    };
    toggle_bitmask_row_generic(state, player_idx, id);
    true
}

/// Generic bitmask toggle for `BitmaskBinding::Generic` bindings. Verifies
/// the focused row matches `id`, computes the target bit via
/// `writeback.bit_mapping.bit_for_choice`, flips it through
/// `init.get_active`/`init.set_active`, projects the resulting bits onto
/// the in-memory profile via `writeback.project_to_profile`, and
/// (conditionally) persists them for the active side via
/// `writeback.persist_for_side`. Plays the change-value SFX on success.
///
/// Returns `true` when a toggle was applied; `false` when the row was not
/// focused, the binding was not `Generic`, or the choice index produced
/// no bit.
pub(super) fn toggle_bitmask_row_generic(state: &mut State, player_idx: usize, id: RowId) -> bool {
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
        &mut state.player_profiles[idx],
        stored,
    );

    let (should_persist, side) = persist_ctx(idx);
    if should_persist {
        (writeback.persist_for_side)(side, &state.player_profiles[idx]);
    }

    if writeback.sync_visibility {
        sync_selected_rows_with_visibility(state, session_active_players());
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
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
