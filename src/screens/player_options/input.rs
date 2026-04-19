use super::*;

pub fn update(state: &mut State, dt: f32, asset_manager: &AssetManager) -> Option<ScreenAction> {
    // Keep options-screen noteskin previews on a stable clock.
    // ITG/SL preview actors are not driven by selected chart BPM, so tying this to song BPM
    // makes beat-based skins (e.g. cel) appear too fast/slow depending on the selected chart.
    const PREVIEW_BPM: f32 = 120.0;
    state.preview_time += dt;
    state.preview_beat += dt * (PREVIEW_BPM / 60.0);
    let active = session_active_players();
    let now = Instant::now();
    let arcade_style = crate::config::get().arcade_options_navigation;
    let mut pending_action: Option<ScreenAction> = None;
    sync_selected_rows_with_visibility(state, active);

    // Hold-to-scroll per player.
    for player_idx in active_player_indices(active) {
        let (Some(direction), Some(held_since), Some(last_scrolled_at)) = (
            state.nav_key_held_direction[player_idx],
            state.nav_key_held_since[player_idx],
            state.nav_key_last_scrolled_at[player_idx],
        ) else {
            continue;
        };
        if now.duration_since(held_since) <= NAV_INITIAL_HOLD_DELAY
            || now.duration_since(last_scrolled_at) < NAV_REPEAT_SCROLL_INTERVAL
        {
            continue;
        }

        if state.pane().row_map.is_empty() {
            continue;
        }
        match direction {
            NavDirection::Up => {
                move_selection_vertical(state, asset_manager, active, player_idx, NavDirection::Up);
            }
            NavDirection::Down => {
                move_selection_vertical(
                    state,
                    asset_manager,
                    active,
                    player_idx,
                    NavDirection::Down,
                );
            }
            NavDirection::Left => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, -1) {
                    apply_choice_delta(state, asset_manager, player_idx, -1);
                }
            }
            NavDirection::Right => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, 1) {
                    apply_choice_delta(state, asset_manager, player_idx, 1);
                }
            }
        }
        state.nav_key_last_scrolled_at[player_idx] = Some(now);
    }

    if arcade_style {
        for player_idx in active_player_indices(active) {
            let action = repeat_held_arcade_start(state, asset_manager, active, player_idx, now);
            if pending_action.is_none() {
                pending_action = action;
            }
        }
    }

    match state.pane_transition {
        PaneTransition::None => {}
        PaneTransition::FadingOut { target, t } => {
            if PANE_FADE_SECONDS <= 0.0 {
                apply_pane(state, target);
                state.pane_transition = PaneTransition::None;
            } else {
                let next_t = (t + dt / PANE_FADE_SECONDS).min(1.0);
                if next_t >= 1.0 {
                    apply_pane(state, target);
                    state.pane_transition = PaneTransition::FadingIn { t: 0.0 };
                } else {
                    state.pane_transition = PaneTransition::FadingOut { target, t: next_t };
                }
            }
        }
        PaneTransition::FadingIn { t } => {
            if PANE_FADE_SECONDS <= 0.0 {
                state.pane_transition = PaneTransition::None;
            } else {
                let next_t = (t + dt / PANE_FADE_SECONDS).min(1.0);
                if next_t >= 1.0 {
                    state.pane_transition = PaneTransition::None;
                } else {
                    state.pane_transition = PaneTransition::FadingIn { t: next_t };
                }
            }
        }
    }

    // Advance help reveal timers.
    for player_idx in active_player_indices(active) {
        state.help_anim_time[player_idx] += dt;
    }

    // If either player is on the Combo Font row, tick the preview combo once per second.
    let mut combo_row_active = false;
    for player_idx in active_player_indices(active) {
        if let Some(row) = state
            .pane()
            .row_map
            .display_order()
            .get(state.pane().selected_row[player_idx])
            .and_then(|&id| state.pane().row_map.get(id))
            && row.id == RowId::ComboFont
        {
            combo_row_active = true;
            break;
        }
    }
    if combo_row_active {
        state.combo_preview_elapsed += dt;
        if state.combo_preview_elapsed >= 1.0 {
            state.combo_preview_elapsed -= 1.0;
            state.combo_preview_count = state.combo_preview_count.saturating_add(1);
        }
    } else {
        state.combo_preview_elapsed = 0.0;
    }

    // Row frame tweening: mimic ScreenOptions::PositionRows() + OptionRow::SetDestination()
    // so rows slide smoothly as the visible window scrolls.
    let total_rows = state.pane().row_map.len();
    let (first_row_center_y, row_step) = row_layout_params();
    if total_rows == 0 {
        state.pane_mut().row_tweens.clear();
    } else if state.pane().row_tweens.len() != total_rows {
        state.pane_mut().row_tweens = init_row_tweens(
            &state.pane().row_map,
            state.pane().selected_row,
            active,
            state.hide_active_mask,
            state.error_bar_active_mask,
            state.allow_per_player_global_offsets,
        );
    } else {
        let visibility = row_visibility(
            &state.pane().row_map,
            active,
            state.hide_active_mask,
            state.error_bar_active_mask,
            state.allow_per_player_global_offsets,
        );
        let visible_rows = count_visible_rows(&state.pane().row_map, visibility);
        if visible_rows == 0 {
            let y = first_row_center_y - row_step * 0.5;
            for tw in &mut state.pane_mut().row_tweens {
                let cur_y = tw.y();
                let cur_a = tw.a();
                if (y - tw.to_y).abs() > 0.01 || tw.to_a != 0.0 {
                    tw.from_y = cur_y;
                    tw.from_a = cur_a;
                    tw.to_y = y;
                    tw.to_a = 0.0;
                    tw.t = 0.0;
                }
                if tw.t < 1.0 {
                    if ROW_TWEEN_SECONDS > 0.0 {
                        tw.t = (tw.t + dt / ROW_TWEEN_SECONDS).min(1.0);
                    } else {
                        tw.t = 1.0;
                    }
                }
            }
        } else {
            let selected_visible = std::array::from_fn(|player_idx| {
                let row_idx =
                    state.pane().selected_row[player_idx].min(total_rows.saturating_sub(1));
                row_to_visible_index(&state.pane().row_map, row_idx, visibility).unwrap_or(0)
            });
            let w = compute_row_window(visible_rows, selected_visible, active);
            let mid_pos = (VISIBLE_ROWS as f32) * 0.5 - 0.5;
            let bottom_pos = (VISIBLE_ROWS as f32) - 0.5;
            let measure_counter_anchor_visible_idx = parent_anchor_visible_index(
                &state.pane().row_map,
                RowId::MeasureCounter,
                visibility,
            );
            let judgment_tilt_anchor_visible_idx =
                parent_anchor_visible_index(&state.pane().row_map, RowId::JudgmentTilt, visibility);
            let error_bar_anchor_visible_idx =
                parent_anchor_visible_index(&state.pane().row_map, RowId::ErrorBar, visibility);
            let hide_anchor_visible_idx =
                parent_anchor_visible_index(&state.pane().row_map, RowId::Hide, visibility);
            let mut visible_idx = 0i32;
            for i in 0..total_rows {
                let visible = is_row_visible(&state.pane().row_map, i, visibility);
                let (f_pos, hidden) = if visible {
                    let ii = visible_idx;
                    visible_idx += 1;
                    f_pos_for_visible_idx(ii, w, mid_pos, bottom_pos)
                } else {
                    let anchor =
                        state.pane().row_map.get_at(i).and_then(
                            |row| match conditional_row_parent(row.id) {
                                Some(RowId::MeasureCounter) => measure_counter_anchor_visible_idx,
                                Some(RowId::JudgmentTilt) => judgment_tilt_anchor_visible_idx,
                                Some(RowId::ErrorBar) => error_bar_anchor_visible_idx,
                                Some(RowId::Hide) => hide_anchor_visible_idx,
                                _ => None,
                            },
                        );
                    if let Some(anchor_idx) = anchor {
                        let (anchor_f_pos, _) =
                            f_pos_for_visible_idx(anchor_idx, w, mid_pos, bottom_pos);
                        (anchor_f_pos, true)
                    } else {
                        (-0.5, true)
                    }
                };

                let dest_y = first_row_center_y + row_step * f_pos;
                let dest_a = if hidden { 0.0 } else { 1.0 };

                let tw = &mut state.pane_mut().row_tweens[i];
                let cur_y = tw.y();
                let cur_a = tw.a();
                if (dest_y - tw.to_y).abs() > 0.01 || dest_a != tw.to_a {
                    tw.from_y = cur_y;
                    tw.from_a = cur_a;
                    tw.to_y = dest_y;
                    tw.to_a = dest_a;
                    tw.t = 0.0;
                }
                if tw.t < 1.0 {
                    if ROW_TWEEN_SECONDS > 0.0 {
                        tw.t = (tw.t + dt / ROW_TWEEN_SECONDS).min(1.0);
                    } else {
                        tw.t = 1.0;
                    }
                }
            }
        }
    }

    // Reset help reveal and play SFX when a player changes rows.
    for player_idx in active_player_indices(active) {
        if state.pane().selected_row[player_idx] == state.pane().prev_selected_row[player_idx] {
            continue;
        }
        match state.nav_key_held_direction[player_idx] {
            Some(NavDirection::Up) => audio::play_sfx("assets/sounds/prev_row.ogg"),
            Some(NavDirection::Down) => audio::play_sfx("assets/sounds/next_row.ogg"),
            _ => audio::play_sfx("assets/sounds/next_row.ogg"),
        }

        state.help_anim_time[player_idx] = 0.0;
        state.pane_mut().prev_selected_row[player_idx] = state.pane().selected_row[player_idx];
    }

    // Retarget cursor tween destinations to match current selection and row destinations.
    for player_idx in active_player_indices(active) {
        let Some((to_x, to_y, to_w, to_h)) =
            cursor_dest_for_player(state, asset_manager, player_idx)
        else {
            continue;
        };

        let to_rect = CursorRect::new(to_x, to_y, to_w, to_h);
        let needs_cursor_init = !state.pane().cursor_initialized[player_idx];
        if needs_cursor_init {
            let pane = state.pane_mut();
            pane.cursor_initialized[player_idx] = true;
            pane.cursor_from[player_idx] = to_rect;
            pane.cursor_to[player_idx] = to_rect;
            pane.cursor_t[player_idx] = 1.0;
        } else {
            let cur_to = state.pane().cursor_to[player_idx];
            let dx = (to_rect.x - cur_to.x).abs();
            let dy = (to_rect.y - cur_to.y).abs();
            let dw = (to_rect.w - cur_to.w).abs();
            let dh = (to_rect.h - cur_to.h).abs();
            if dx > 0.01 || dy > 0.01 || dw > 0.01 || dh > 0.01 {
                let pane = state.pane();
                let t = pane.cursor_t[player_idx].clamp(0.0, 1.0);
                let cur_rect =
                    CursorRect::lerp(pane.cursor_from[player_idx], pane.cursor_to[player_idx], t);

                let pane = state.pane_mut();
                pane.cursor_from[player_idx] = cur_rect;
                pane.cursor_to[player_idx] = to_rect;
                pane.cursor_t[player_idx] = 0.0;
            }
        }
    }

    // Advance cursor tween.
    for player_idx in [P1, P2] {
        if state.pane().cursor_t[player_idx] < 1.0 {
            if CURSOR_TWEEN_SECONDS > 0.0 {
                state.pane_mut().cursor_t[player_idx] =
                    (state.pane().cursor_t[player_idx] + dt / CURSOR_TWEEN_SECONDS).min(1.0);
            } else {
                state.pane_mut().cursor_t[player_idx] = 1.0;
            }
        }
    }

    pending_action
}

pub fn on_nav_press(state: &mut State, player_idx: usize, dir: NavDirection) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.scroll_focus_player = idx;
    state.nav_key_held_direction[idx] = Some(dir);
    state.nav_key_held_since[idx] = Some(Instant::now());
    state.nav_key_last_scrolled_at[idx] = Some(Instant::now());
}

pub fn on_nav_release(state: &mut State, player_idx: usize, dir: NavDirection) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    if state.nav_key_held_direction[idx] == Some(dir) {
        state.nav_key_held_direction[idx] = None;
        state.nav_key_held_since[idx] = None;
        state.nav_key_last_scrolled_at[idx] = None;
    }
}

#[inline(always)]
pub(super) fn on_start_press(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let now = Instant::now();
    state.start_held_since[idx] = Some(now);
    state.start_last_triggered_at[idx] = Some(now);
}

#[inline(always)]
pub(super) fn clear_start_hold(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.start_held_since[idx] = None;
    state.start_last_triggered_at[idx] = None;
}

pub(super) fn focus_exit_row(state: &mut State, active: [bool; PLAYER_SLOTS], player_idx: usize) {
    if state.pane().row_map.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.pane_mut().selected_row[idx] = state.pane().row_map.len().saturating_sub(1);
    state.pane_mut().arcade_row_focus[idx] =
        row_allows_arcade_next_row(state, state.pane().selected_row[idx]);
    sync_selected_rows_with_visibility(state, active);
}

#[inline(always)]
pub(super) fn finish_start_without_action(
    state: &mut State,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    should_focus_exit: bool,
) -> Option<ScreenAction> {
    if should_focus_exit {
        focus_exit_row(state, active, player_idx);
    }
    None
}

pub(super) fn handle_nav_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    dir: NavDirection,
    pressed: bool,
) {
    if !active[player_idx] || state.pane().row_map.is_empty() {
        return;
    }
    if pressed {
        sync_selected_rows_with_visibility(state, active);
        match dir {
            NavDirection::Up => {
                move_selection_vertical(state, asset_manager, active, player_idx, NavDirection::Up)
            }
            NavDirection::Down => move_selection_vertical(
                state,
                asset_manager,
                active,
                player_idx,
                NavDirection::Down,
            ),
            NavDirection::Left => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, -1) {
                    apply_choice_delta(state, asset_manager, player_idx, -1);
                    if arcade_row_uses_choice_focus(state, player_idx) {
                        state.pane_mut().arcade_row_focus[player_idx.min(PLAYER_SLOTS - 1)] = false;
                    }
                }
            }
            NavDirection::Right => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, 1) {
                    apply_choice_delta(state, asset_manager, player_idx, 1);
                    if arcade_row_uses_choice_focus(state, player_idx) {
                        state.pane_mut().arcade_row_focus[player_idx.min(PLAYER_SLOTS - 1)] = false;
                    }
                }
            }
        }
        on_nav_press(state, player_idx, dir);
    } else {
        on_nav_release(state, player_idx, dir);
    }
}

#[inline(always)]
pub(super) fn clear_nav_hold(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.nav_key_held_direction[idx] = None;
    state.nav_key_held_since[idx] = None;
    state.nav_key_last_scrolled_at[idx] = None;
}

#[inline(always)]
pub(super) fn player_side_for_idx(player_idx: usize) -> crate::game::profile::PlayerSide {
    if player_idx == P2 {
        crate::game::profile::PlayerSide::P2
    } else {
        crate::game::profile::PlayerSide::P1
    }
}

pub(super) fn handle_arcade_start_press(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    repeated: bool,
) -> Option<ScreenAction> {
    if screen_input::menu_lr_both_held(&state.menu_lr_chord, player_side_for_idx(player_idx)) {
        handle_arcade_prev_event(state, asset_manager, active, player_idx);
        return None;
    }
    if repeated && !state.pane().row_map.is_empty() {
        let idx = player_idx.min(PLAYER_SLOTS - 1);
        let row_idx =
            state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
        if row_idx + 1 == state.pane().row_map.len() {
            return None;
        }
    }
    handle_arcade_start_event(state, asset_manager, active, player_idx)
}

pub(super) fn repeat_held_arcade_start(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    now: Instant,
) -> Option<ScreenAction> {
    if !active[player_idx] {
        clear_start_hold(state, player_idx);
        return None;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let (Some(held_since), Some(last_triggered_at)) = (
        state.start_held_since[idx],
        state.start_last_triggered_at[idx],
    ) else {
        return None;
    };
    if now.duration_since(held_since) <= NAV_INITIAL_HOLD_DELAY
        || now.duration_since(last_triggered_at) < NAV_REPEAT_SCROLL_INTERVAL
    {
        return None;
    }
    state.start_last_triggered_at[idx] = Some(now);
    handle_arcade_start_press(state, asset_manager, active, player_idx, true)
}

pub(super) fn move_arcade_horizontal_focus(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) -> bool {
    if delta == 0 || state.pane().row_map.is_empty() {
        return false;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
    let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))
    else {
        return false;
    };
    let row_supports_inline = row_supports_inline_nav(row);
    let num_choices = row.choices.len();
    let current_choice = row
        .selected_choice_index
        .get(idx)
        .copied()
        .unwrap_or(0)
        .min(num_choices.saturating_sub(1));
    if !row_allows_arcade_next_row(state, row_idx) {
        return false;
    }
    if row_supports_inline {
        apply_choice_delta(state, asset_manager, idx, delta);
        return true;
    }
    if num_choices <= 1 {
        return false;
    }
    if state.pane().arcade_row_focus[idx] {
        if delta < 0 {
            return false;
        }
        state.pane_mut().arcade_row_focus[idx] = false;
        if current_choice == 0 {
            audio::play_sfx("assets/sounds/change_value.ogg");
        } else {
            change_choice_for_player(state, asset_manager, idx, -(current_choice as isize));
        }
        return true;
    }
    if delta < 0 {
        if current_choice == 0 {
            state.pane_mut().arcade_row_focus[idx] = true;
            audio::play_sfx("assets/sounds/change_value.ogg");
            return true;
        }
        change_choice_for_player(state, asset_manager, idx, -1);
        return true;
    }
    if current_choice + 1 >= num_choices {
        return false;
    }
    change_choice_for_player(state, asset_manager, idx, 1);
    true
}

pub(super) fn handle_arcade_prev_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
) {
    if !active[player_idx] || state.pane().row_map.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let prev_row = state.pane().selected_row[idx];
    clear_nav_hold(state, player_idx);
    move_selection_vertical(state, asset_manager, active, player_idx, NavDirection::Up);
    if state.pane().selected_row[idx] != prev_row {
        audio::play_sfx("assets/sounds/prev_row.ogg");
        state.help_anim_time[idx] = 0.0;
        state.pane_mut().prev_selected_row[idx] = state.pane().selected_row[idx];
    }
}

pub(super) fn handle_arcade_start_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
) -> Option<ScreenAction> {
    if !active[player_idx] {
        return None;
    }
    sync_selected_rows_with_visibility(state, active);
    let num_rows = state.pane().row_map.len();
    if num_rows == 0 {
        return None;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.pane().selected_row[idx].min(num_rows.saturating_sub(1));
    if row_index + 1 == num_rows {
        state.pane_mut().arcade_row_focus[idx] = row_allows_arcade_next_row(state, row_index);
        return handle_start_event(state, asset_manager, active, idx);
    }
    if arcade_row_uses_choice_focus(state, idx) && !state.pane().arcade_row_focus[idx] {
        let action = handle_start_event(state, asset_manager, active, idx);
        state.pane_mut().arcade_row_focus[idx] = row_allows_arcade_next_row(state, row_index);
        return action;
    }
    move_selection_vertical(state, asset_manager, active, idx, NavDirection::Down);
    state.pane_mut().arcade_row_focus[idx] =
        row_allows_arcade_next_row(state, state.pane().selected_row[idx]);
    None
}

pub(super) fn handle_start_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
) -> Option<ScreenAction> {
    if !active[player_idx] {
        return None;
    }
    sync_selected_rows_with_visibility(state, active);
    let num_rows = state.pane().row_map.len();
    if num_rows == 0 {
        return None;
    }
    let row_index = state.pane().selected_row[player_idx].min(num_rows.saturating_sub(1));
    let should_focus_exit = state.current_pane == OptionsPane::Main && row_index + 1 < num_rows;
    let row = state
        .pane()
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.pane().row_map.get(id))?;
    let id = row.id;
    let row_supports_inline = row_supports_inline_nav(row);
    let row_toggles = row_toggles_with_start(row);
    if row_supports_inline {
        let changed = commit_inline_focus_selection(state, asset_manager, player_idx, row_index);
        if changed && !row_toggles {
            change_choice_for_player(state, asset_manager, player_idx, 0);
            return finish_start_without_action(state, active, player_idx, should_focus_exit);
        }
    }
    if super::choice::dispatch_behavior_toggle(state, player_idx, id) {
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if row_index == num_rows.saturating_sub(1)
        && let Some(what_comes_next_row) = state
            .pane()
            .row_map
            .display_order()
            .get(num_rows.saturating_sub(2))
            .and_then(|&id| state.pane().row_map.get(id))
        && what_comes_next_row.id == RowId::WhatComesNext
    {
        let choice_idx = what_comes_next_row.selected_choice_index[player_idx];
        if let Some(choice) = what_comes_next_row.choices.get(choice_idx) {
            let gameplay = tr("PlayerOptions", "WhatComesNextGameplay");
            let advanced = tr("PlayerOptions", "WhatComesNextAdvancedModifiers");
            let uncommon = tr("PlayerOptions", "WhatComesNextUncommonModifiers");
            let main_mods = tr("PlayerOptions", "WhatComesNextMainModifiers");
            let choose_different = choose_different_screen_label(state.return_screen);
            let choice_str = choice.as_str();
            if choice_str == gameplay.as_ref() {
                audio::play_sfx("assets/sounds/start.ogg");
                return Some(ScreenAction::Navigate(Screen::Gameplay));
            } else if choice_str == choose_different {
                audio::play_sfx("assets/sounds/start.ogg");
                return Some(ScreenAction::Navigate(state.return_screen));
            } else if choice_str == advanced.as_ref() {
                switch_to_pane(state, OptionsPane::Advanced);
            } else if choice_str == uncommon.as_ref() {
                switch_to_pane(state, OptionsPane::Uncommon);
            } else if choice_str == main_mods.as_ref() {
                switch_to_pane(state, OptionsPane::Main);
            }
        }
    }
    finish_start_without_action(state, active, player_idx, should_focus_exit)
}

pub fn handle_input(
    state: &mut State,
    asset_manager: &AssetManager,
    ev: &InputEvent,
) -> ScreenAction {
    let active = session_active_players();
    let dedicated_three_key = screen_input::dedicated_three_key_nav_enabled();
    let arcade_style = crate::config::get().arcade_options_navigation;
    if arcade_options_navigation_active() || dedicated_three_key {
        screen_input::track_menu_lr_chord(&mut state.menu_lr_chord, ev);
    }
    let three_key_action = (!dedicated_three_key)
        .then(|| screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev))
        .flatten();
    if state.pane_transition.is_active() {
        if let Some((side, screen_input::ThreeKeyMenuAction::Cancel)) = three_key_action {
            let player_idx = screen_input::player_side_ix(side);
            if active[player_idx] {
                return ScreenAction::Navigate(state.return_screen);
            }
        }
        return match ev.action {
            VirtualAction::p1_back if ev.pressed && active[P1] => {
                ScreenAction::Navigate(state.return_screen)
            }
            VirtualAction::p2_back if ev.pressed && active[P2] => {
                ScreenAction::Navigate(state.return_screen)
            }
            _ => ScreenAction::None,
        };
    }
    if let Some((side, nav)) = three_key_action {
        let player_idx = screen_input::player_side_ix(side);
        if !active[player_idx] {
            return ScreenAction::None;
        }
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
                handle_nav_event(
                    state,
                    asset_manager,
                    active,
                    player_idx,
                    NavDirection::Up,
                    true,
                );
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Next => {
                handle_nav_event(
                    state,
                    asset_manager,
                    active,
                    player_idx,
                    NavDirection::Down,
                    true,
                );
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Confirm => {
                clear_nav_hold(state, player_idx);
                if let Some(action) = handle_start_event(state, asset_manager, active, player_idx) {
                    return action;
                }
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Cancel => {
                clear_nav_hold(state, player_idx);
                ScreenAction::Navigate(state.return_screen)
            }
        };
    }
    match ev.action {
        VirtualAction::p1_back if ev.pressed && active[P1] => {
            return ScreenAction::Navigate(state.return_screen);
        }
        VirtualAction::p2_back if ev.pressed && active[P2] => {
            return ScreenAction::Navigate(state.return_screen);
        }
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Up,
                ev.pressed,
            );
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Down,
                ev.pressed,
            );
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Left,
                ev.pressed,
            );
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Right,
                ev.pressed,
            );
        }
        VirtualAction::p1_start => {
            if !ev.pressed {
                clear_start_hold(state, P1);
                return ScreenAction::None;
            }
            if arcade_style {
                on_start_press(state, P1);
                if let Some(action) =
                    handle_arcade_start_press(state, asset_manager, active, P1, false)
                {
                    return action;
                }
                return ScreenAction::None;
            }
            if let Some(action) = handle_start_event(state, asset_manager, active, P1) {
                return action;
            }
        }
        VirtualAction::p1_select if ev.pressed && arcade_style => {
            handle_arcade_prev_event(state, asset_manager, active, P1);
            return ScreenAction::None;
        }
        VirtualAction::p2_up | VirtualAction::p2_menu_up => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Up,
                ev.pressed,
            );
        }
        VirtualAction::p2_down | VirtualAction::p2_menu_down => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Down,
                ev.pressed,
            );
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Left,
                ev.pressed,
            );
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Right,
                ev.pressed,
            );
        }
        VirtualAction::p2_start => {
            if !ev.pressed {
                clear_start_hold(state, P2);
                return ScreenAction::None;
            }
            if arcade_style {
                on_start_press(state, P2);
                if let Some(action) =
                    handle_arcade_start_press(state, asset_manager, active, P2, false)
                {
                    return action;
                }
                return ScreenAction::None;
            }
            if let Some(action) = handle_start_event(state, asset_manager, active, P2) {
                return action;
            }
        }
        VirtualAction::p2_select if ev.pressed && arcade_style => {
            handle_arcade_prev_event(state, asset_manager, active, P2);
            return ScreenAction::None;
        }
        _ => {}
    }
    ScreenAction::None
}
