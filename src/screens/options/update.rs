use super::*;

/// Refresh cached translated labels when the UI language changes.
pub(super) fn sync_i18n_cache(state: &mut State) {
    let rev = crate::assets::i18n::revision();
    if state.i18n_revision == rev {
        return;
    }
    state.i18n_revision = rev;
    state.display_mode_choices = build_display_mode_choices(&state.monitor_specs);
    state.software_thread_labels = software_thread_choice_labels(&state.software_thread_choices);
    let (si_packs, si_filters) = score_import_pack_options();
    state.score_import_pack_choices = si_packs;
    state.score_import_pack_filters = si_filters;
    let (sp_packs, sp_filters) = sync_pack_options();
    state.sync_pack_choices = sp_packs;
    state.sync_pack_filters = sp_filters;
    #[cfg(target_os = "linux")]
    {
        state.linux_backend_choices = build_linux_backend_choices();
    }
    clear_render_cache(state);
}

pub(super) fn clear_navigation_holds(state: &mut State) {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.nav_key_last_scrolled_at = None;
    state.nav_lr_held_direction = None;
    state.nav_lr_held_since = None;
    state.nav_lr_last_adjusted_at = None;
}

pub fn sync_video_renderer(state: &mut State, renderer: BackendType) {
    state.video_renderer_at_load = renderer;
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::VideoRenderer,
    ) {
        *slot = backend_to_renderer_choice_index(renderer);
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_display_mode(
    state: &mut State,
    mode: DisplayMode,
    fullscreen_type: FullscreenType,
    monitor: usize,
    monitor_count: usize,
) {
    state.display_mode_at_load = mode;
    state.display_monitor_at_load = monitor;
    set_display_mode_row_selection(state, monitor_count, mode, monitor);
    let target_type = match mode {
        DisplayMode::Fullscreen(ft) => ft,
        DisplayMode::Windowed => fullscreen_type,
    };
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::FullscreenType,
    ) {
        *slot = target_type.choice_index();
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_display_resolution(state: &mut State, width: u32, height: u32) {
    sync_display_aspect_ratio(state, width, height);
    rebuild_resolution_choices(state, width, height);
    state.display_width_at_load = width;
    state.display_height_at_load = height;
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_show_stats_mode(state: &mut State, mode: u8) {
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::ShowStats,
        mode.min(3) as usize,
    );
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_translated_titles(state: &mut State, enabled: bool) {
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowNativeLanguage,
        translated_titles_choice_index(enabled),
    );
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_max_fps(state: &mut State, max_fps: u16) {
    let had_explicit_cap = state.max_fps_at_load != 0;
    state.max_fps_at_load = max_fps;
    set_max_fps_enabled_choice(state, max_fps != 0);
    if max_fps != 0 || !had_explicit_cap {
        seed_max_fps_value_choice(state, max_fps);
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_vsync(state: &mut State, enabled: bool) {
    state.vsync_at_load = enabled;
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::VSync,
    ) {
        *slot = yes_no_choice_index(enabled);
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_high_dpi(state: &mut State, enabled: bool) {
    state.high_dpi_at_load = enabled;
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::HighDpi,
        yes_no_choice_index(enabled),
    );
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_present_mode_policy(state: &mut State, mode: PresentModePolicy) {
    state.present_mode_policy_at_load = mode;
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::PresentMode,
    ) {
        *slot = mode.choice_index();
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn open_input_submenu(state: &mut State) {
    state.view = OptionsView::Submenu(SubmenuKind::Input);
    state.pending_submenu_kind = None;
    state.pending_submenu_parent_kind = None;
    state.submenu_parent_kind = None;
    state.submenu_transition = SubmenuTransition::None;
    state.submenu_fade_t = 0.0;
    state.content_alpha = 1.0;
    state.sub_selected = 0;
    state.sub_prev_selected = 0;
    state.sub_inline_x = f32::NAN;
    sync_submenu_cursor_indices(state);
    state.cursor_initialized = false;
    state.cursor_t = 1.0;
    state.row_tweens.clear();
    state.graphics_prev_visible_rows.clear();
    state.advanced_prev_visible_rows.clear();
    state.select_music_prev_visible_rows.clear();
    clear_navigation_holds(state);
    clear_render_cache(state);
}

pub fn update(state: &mut State, dt: f32, asset_manager: &AssetManager) -> Option<ScreenAction> {
    if state.reload_ui.is_some() {
        let done = {
            let reload = state.reload_ui.as_mut().unwrap();
            poll_reload_ui(reload);
            reload.done
        };
        if done {
            state.reload_ui = None;
            refresh_score_import_pack_options(state);
        }
        return None;
    }
    if let Some(score_import) = state.score_import_ui.as_mut() {
        poll_score_import_ui(score_import, dt);
        if score_import.done
            && score_import
                .done_since
                .is_some_and(|at| at.elapsed().as_secs_f32() >= SCORE_IMPORT_DONE_OVERLAY_SECONDS)
        {
            state.score_import_ui = None;
        }
        return None;
    }
    if shared_pack_sync::poll(&mut state.pack_sync_overlay) {
        return None;
    }

    sync_i18n_cache(state);

    let mut pending_action: Option<ScreenAction> = None;
    // ------------------------- local submenu fade ------------------------- //
    match state.submenu_transition {
        SubmenuTransition::None => {
            state.content_alpha = 1.0;
        }
        SubmenuTransition::FadeOutToSubmenu => {
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = 1.0 - state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                // Apply deferred settings before leaving the submenu.
                if matches!(state.view, OptionsView::Submenu(SubmenuKind::InputBackend))
                    && let Some(enabled) = state.pending_dedicated_menu_buttons.take()
                {
                    config::update_only_dedicated_menu_buttons(enabled);
                }
                // Switch view to the target submenu, then fade it in.
                let target_kind = state.pending_submenu_kind.unwrap_or(SubmenuKind::System);
                state.view = OptionsView::Submenu(target_kind);
                state.pending_submenu_kind = None;
                state.submenu_parent_kind = state.pending_submenu_parent_kind.take();
                state.sub_selected = 0;
                state.sub_prev_selected = 0;
                state.sub_inline_x = f32::NAN;
                sync_submenu_cursor_indices(state);
                state.cursor_initialized = false;
                state.cursor_t = 1.0;
                state.row_tweens.clear();
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
                state.nav_key_last_scrolled_at = None;
                state.nav_lr_held_direction = None;
                state.nav_lr_held_since = None;
                state.nav_lr_last_adjusted_at = None;
                state.submenu_transition = SubmenuTransition::FadeInSubmenu;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 0.0;
            }
        }
        SubmenuTransition::FadeInSubmenu => {
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                state.submenu_transition = SubmenuTransition::None;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 1.0;
            }
        }
        SubmenuTransition::FadeOutToMain => {
            let leaving_graphics =
                matches!(state.view, OptionsView::Submenu(SubmenuKind::Graphics));
            let (
                desired_renderer,
                desired_display_mode,
                desired_resolution,
                desired_monitor,
                desired_vsync,
                desired_present_mode_policy,
                desired_max_fps,
                desired_high_dpi,
            ) = if leaving_graphics {
                let vsync = get_choice_by_id(
                    &state.sub[SubmenuKind::Graphics].choice_indices,
                    GRAPHICS_OPTIONS_ROWS,
                    SubRowId::VSync,
                )
                .is_none_or(yes_no_from_choice);
                (
                    Some(selected_video_renderer(state)),
                    Some(selected_display_mode(state)),
                    Some(selected_resolution(state)),
                    Some(selected_display_monitor(state)),
                    Some(vsync),
                    Some(selected_present_mode_policy(state)),
                    Some(selected_max_fps(state)),
                    Some(selected_high_dpi(state)),
                )
            } else {
                (None, None, None, None, None, None, None, None)
            };
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = 1.0 - state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                // Return to the main options list and fade it in.
                state.view = OptionsView::Main;
                state.pending_submenu_kind = None;
                state.pending_submenu_parent_kind = None;
                state.submenu_parent_kind = None;
                state.cursor_initialized = false;
                state.cursor_t = 1.0;
                state.row_tweens.clear();
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
                state.nav_key_last_scrolled_at = None;
                state.nav_lr_held_direction = None;
                state.nav_lr_held_since = None;
                state.nav_lr_last_adjusted_at = None;
                state.submenu_transition = SubmenuTransition::FadeInMain;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 0.0;

                let mut renderer_change: Option<BackendType> = None;
                let mut display_mode_change: Option<DisplayMode> = None;
                let mut resolution_change: Option<(u32, u32)> = None;
                let mut monitor_change: Option<usize> = None;
                let mut vsync_change: Option<bool> = None;
                let mut present_mode_policy_change: Option<PresentModePolicy> = None;
                let mut max_fps_change: Option<u16> = None;
                let mut high_dpi_change: Option<bool> = None;

                if let Some(renderer) = desired_renderer
                    && renderer != state.video_renderer_at_load
                {
                    renderer_change = Some(renderer);
                }
                if let Some(display_mode) = desired_display_mode
                    && display_mode != state.display_mode_at_load
                {
                    display_mode_change = Some(display_mode);
                }
                if let Some(monitor) = desired_monitor
                    && monitor != state.display_monitor_at_load
                {
                    monitor_change = Some(monitor);
                }
                if let Some((w, h)) = desired_resolution
                    && (w != state.display_width_at_load || h != state.display_height_at_load)
                {
                    resolution_change = Some((w, h));
                }
                if let Some(vsync) = desired_vsync
                    && vsync != state.vsync_at_load
                {
                    vsync_change = Some(vsync);
                }
                if let Some(policy) = desired_present_mode_policy
                    && policy != state.present_mode_policy_at_load
                {
                    present_mode_policy_change = Some(policy);
                }
                if let Some(max_fps) = desired_max_fps
                    && max_fps != state.max_fps_at_load
                {
                    max_fps_change = Some(max_fps);
                }
                if let Some(high_dpi) = desired_high_dpi
                    && high_dpi != state.high_dpi_at_load
                {
                    high_dpi_change = Some(high_dpi);
                    if resolution_change.is_none() {
                        resolution_change = desired_resolution;
                    }
                }

                if renderer_change.is_some()
                    || display_mode_change.is_some()
                    || monitor_change.is_some()
                    || resolution_change.is_some()
                    || vsync_change.is_some()
                    || present_mode_policy_change.is_some()
                    || max_fps_change.is_some()
                    || high_dpi_change.is_some()
                {
                    pending_action = Some(ScreenAction::ChangeGraphics {
                        renderer: renderer_change,
                        display_mode: display_mode_change,
                        monitor: monitor_change,
                        resolution: resolution_change,
                        vsync: vsync_change,
                        present_mode_policy: present_mode_policy_change,
                        max_fps: max_fps_change,
                        high_dpi: high_dpi_change,
                    });
                }
            }
        }
        SubmenuTransition::FadeInMain => {
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                state.submenu_transition = SubmenuTransition::None;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 1.0;
            }
        }
    }

    // While fading, freeze hold-to-scroll to avoid odd jumps.
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return pending_action;
    }

    if let (Some(direction), Some(held_since), Some(last_scrolled_at)) = (
        state.nav_key_held_direction,
        state.nav_key_held_since,
        state.nav_key_last_scrolled_at,
    ) {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL
        {
            match state.view {
                OptionsView::Main => {
                    let total = ITEMS.len();
                    if total > 0 {
                        let last = total - 1;
                        match direction {
                            NavDirection::Up => {
                                if state.selected > 0 {
                                    state.selected -= 1;
                                }
                            }
                            NavDirection::Down => {
                                if state.selected < last {
                                    state.selected += 1;
                                }
                            }
                        }
                        state.nav_key_last_scrolled_at = Some(now);
                    }
                }
                OptionsView::Submenu(kind) => {
                    move_submenu_selection_vertical(
                        state,
                        asset_manager,
                        kind,
                        direction,
                        NavWrap::Clamp,
                    );
                    state.nav_key_last_scrolled_at = Some(now);
                }
            }
        }
    }

    if let (Some(delta_lr), Some(held_since), Some(last_adjusted)) = (
        state.nav_lr_held_direction,
        state.nav_lr_held_since,
        state.nav_lr_last_adjusted_at,
    ) {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_adjusted) >= NAV_REPEAT_SCROLL_INTERVAL
            && matches!(state.view, OptionsView::Submenu(_))
        {
            let repeat_delta = if on_max_fps_value_row(state) {
                max_fps_hold_delta(delta_lr, now.duration_since(held_since))
            } else {
                delta_lr
            };
            if pending_action.is_none() {
                pending_action =
                    apply_submenu_choice_delta(state, asset_manager, repeat_delta, NavWrap::Clamp);
            } else {
                apply_submenu_choice_delta(state, asset_manager, repeat_delta, NavWrap::Clamp);
            }
            state.nav_lr_last_adjusted_at = Some(now);
        }
    }

    match state.view {
        OptionsView::Main => {
            if state.selected != state.prev_selected {
                audio::play_sfx("assets/sounds/change.ogg");
                state.prev_selected = state.selected;
            }
        }
        OptionsView::Submenu(_) => {
            if state.sub_selected != state.sub_prev_selected {
                audio::play_sfx("assets/sounds/change.ogg");
                state.sub_prev_selected = state.sub_selected;
            }
        }
    }

    let (s, list_x, list_y) = scaled_block_origin_with_margins();
    match state.view {
        OptionsView::Main => {
            update_row_tweens(
                &mut state.row_tweens,
                ITEMS.len(),
                state.selected,
                s,
                list_y,
                dt,
            );
            state.cursor_initialized = false;
            state.graphics_prev_visible_rows.clear();
            state.advanced_prev_visible_rows.clear();
            state.select_music_prev_visible_rows.clear();
        }
        OptionsView::Submenu(kind) => {
            if matches!(kind, SubmenuKind::Graphics) {
                update_graphics_row_tweens(state, s, list_y, dt);
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
            } else if matches!(kind, SubmenuKind::Advanced) {
                update_advanced_row_tweens(state, s, list_y, dt);
                state.graphics_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
            } else if matches!(kind, SubmenuKind::SelectMusic) {
                update_select_music_row_tweens(state, s, list_y, dt);
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
            } else {
                let total_rows = submenu_total_rows(state, kind);
                update_row_tweens(
                    &mut state.row_tweens,
                    total_rows,
                    state.sub_selected,
                    s,
                    list_y,
                    dt,
                );
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
            }
            let list_w = list_w_unscaled() * s;
            if let Some((to_x, to_y, to_w, to_h)) =
                submenu_cursor_dest(state, asset_manager, kind, s, list_x, list_y, list_w)
            {
                let needs_cursor_init = !state.cursor_initialized;
                if needs_cursor_init {
                    state.cursor_initialized = true;
                    state.cursor_from_x = to_x;
                    state.cursor_from_y = to_y;
                    state.cursor_from_w = to_w;
                    state.cursor_from_h = to_h;
                    state.cursor_to_x = to_x;
                    state.cursor_to_y = to_y;
                    state.cursor_to_w = to_w;
                    state.cursor_to_h = to_h;
                    state.cursor_t = 1.0;
                } else {
                    let dx = (to_x - state.cursor_to_x).abs();
                    let dy = (to_y - state.cursor_to_y).abs();
                    let dw = (to_w - state.cursor_to_w).abs();
                    let dh = (to_h - state.cursor_to_h).abs();
                    if dx > 0.01 || dy > 0.01 || dw > 0.01 || dh > 0.01 {
                        let t = state.cursor_t.clamp(0.0, 1.0);
                        let cur_x = (state.cursor_to_x - state.cursor_from_x)
                            .mul_add(t, state.cursor_from_x);
                        let cur_y = (state.cursor_to_y - state.cursor_from_y)
                            .mul_add(t, state.cursor_from_y);
                        let cur_w = (state.cursor_to_w - state.cursor_from_w)
                            .mul_add(t, state.cursor_from_w);
                        let cur_h = (state.cursor_to_h - state.cursor_from_h)
                            .mul_add(t, state.cursor_from_h);
                        state.cursor_from_x = cur_x;
                        state.cursor_from_y = cur_y;
                        state.cursor_from_w = cur_w;
                        state.cursor_from_h = cur_h;
                        state.cursor_to_x = to_x;
                        state.cursor_to_y = to_y;
                        state.cursor_to_w = to_w;
                        state.cursor_to_h = to_h;
                        state.cursor_t = 0.0;
                    }
                }
            } else {
                state.cursor_initialized = false;
            }
        }
    }

    if state.cursor_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_t = (state.cursor_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.cursor_t = 1.0;
        }
    }

    pending_action
}
