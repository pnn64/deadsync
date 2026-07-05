use super::*;
use deadsync_config::machine::clamp_smx_light_brightness_percent;

#[inline(always)]
fn dedicated_menu_buttons_supported(three_key_navigation: bool) -> bool {
    deadsync_input::any_player_has_dedicated_menu_buttons_for_mode(three_key_navigation)
}

#[inline(always)]
const fn dedicated_menu_navigation_label(three_key_navigation: bool) -> &'static str {
    if three_key_navigation {
        "Three Key Menu"
    } else {
        "Five Key Menu"
    }
}

pub fn update_input_debounce_seconds(seconds: f32) {
    let seconds = deadsync_input::clamp_input_debounce_seconds(seconds);
    if update_config_f32(seconds, |cfg| &mut cfg.input_debounce_seconds) {
        deadsync_input::set_input_debounce_seconds(seconds);
    }
}

pub fn update_arcade_options_navigation(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.arcade_options_navigation);
}

pub fn update_delayed_back(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.delayed_back);
}

pub fn update_three_key_navigation(enabled: bool) {
    let dedicated = {
        let mut cfg = lock_config();
        if cfg.three_key_navigation == enabled {
            return;
        }
        cfg.three_key_navigation = enabled;
        if cfg.only_dedicated_menu_buttons && !dedicated_menu_buttons_supported(enabled) {
            warn!(
                "three_key_navigation changed to {} but no player has the required dedicated menu buttons mapped — disabling dedicated-only menu navigation.",
                dedicated_menu_navigation_label(enabled)
            );
            cfg.only_dedicated_menu_buttons = false;
        }
        cfg.only_dedicated_menu_buttons
    };
    deadsync_input::set_only_dedicated_menu_buttons(dedicated);
    save_without_keymaps();
}

pub fn update_use_fsrs(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.use_fsrs);
}

/// Persist the StepManiaX-pad input toggle. The SMX manager and listeners are
/// wired at startup, so this takes effect on the next launch (mirroring the
/// gamepad-backend option).
pub fn update_smx_input(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.smx_input);
}

/// Persist the "DeadSync manages pad config" toggle. When on, the app loop
/// (`apply_smx_managed_preset`) resolves and writes each connected SMX pad's
/// config every non-gameplay frame; this just records the preference.
pub fn update_smx_manages_pad_config(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.smx_manages_pad_config);
}

/// Persist whether SMX pad panels light up during gameplay. Saving the flag is all this
/// needs to do: `App::sync_lights` reads it from config alongside the other lights settings
/// and activates or releases the panel worker accordingly, so there is no separate driver or
/// runtime state to update here.
pub fn update_smx_panel_lights(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.smx_panel_lights);
}

pub fn update_smx_underglow_theme(enabled: bool) {
    if update_config_value(enabled, |cfg| &mut cfg.smx_underglow_theme) && enabled {
        send_smx_underglow_color();
    }
}

/// Persist the underglow GRB wire-order switch and push it to the strip send
/// path, re-sending the current colour so the change shows immediately (the
/// options-page preview also re-sends its test colour on its own).
pub fn update_smx_underglow_grb(grb: bool) {
    if update_config_value(grb, |cfg| &mut cfg.smx_underglow_grb) {
        deadsync_smx::set_platform_lights_grb(grb);
        send_smx_underglow_color();
    }
}

/// Persist the built-in default pad preset (Low/Medium/High). Used as the
/// fallback config flashed to a managed pad when no saved config resolves.
pub fn update_smx_default_pad_config(preset: crate::config::SmxPadPreset) {
    update_config_value(preset, |cfg| &mut cfg.smx_default_pad_config);
}

/// Persist the machine-default pad-light brightness (0..=100). This seeds new
/// player profiles; it does not retroactively change existing profiles, so there
/// is nothing to push to the SDK here (the per-slot live value is resolved from
/// the active profiles by `App::sync_smx_light_brightness`).
pub fn update_smx_default_light_brightness(percent: u8) {
    let percent = clamp_smx_light_brightness_percent(percent);
    update_config_value(percent, |cfg| &mut cfg.smx_default_light_brightness);
}

/// Persist the SMX pad → player serial assignment and push it live to the SDK,
/// which re-orders the device slots so slot 0 = P1, slot 1 = P2. A `None` clears
/// that side (falling back to the hardware jumper). No-op if unchanged.
pub fn update_smx_pad_assignment(p1_serial: Option<String>, p2_serial: Option<String>) {
    {
        let mut p1 = SMX_P1_SERIAL.lock().unwrap();
        let mut p2 = SMX_P2_SERIAL.lock().unwrap();
        if *p1 == p1_serial && *p2 == p2_serial {
            return;
        }
        *p1 = p1_serial.clone();
        *p2 = p2_serial.clone();
    }
    deadsync_smx::set_player_assignment(p1_serial, p2_serial);
    save_without_keymaps();
}

/// Persist the default local profile id per side. `None` means that side
/// defaults to Guest. No-op (and no disk write) when nothing changed.
pub fn update_default_profiles(p1: Option<String>, p2: Option<String>) {
    {
        let mut a = DEFAULT_PROFILE_P1.lock().unwrap();
        let mut b = DEFAULT_PROFILE_P2.lock().unwrap();
        if *a == p1 && *b == p2 {
            return;
        }
        *a = p1;
        *b = p2;
    }
    save_without_keymaps();
}

/// Swap which physical pad is P1 vs P2. Uses the serials currently connected at
/// slot 0 and slot 1 and pins them reversed, so the swap is immediate and works
/// whether or not an assignment was already saved. Returns whether it swapped:
/// `false` (no-op) unless both pads are connected, since a swap is undefined with
/// fewer than two pads.
pub fn swap_smx_pad_assignment() -> bool {
    let [s0, s1] = deadsync_smx::connected_serials();
    if let (Some(a), Some(b)) = (s0, s1) {
        update_smx_pad_assignment(Some(b), Some(a));
        true
    } else {
        false
    }
}

pub fn update_only_dedicated_menu_buttons(enabled: bool) {
    let enabled = {
        let mut cfg = lock_config();
        let enabled = if enabled && !dedicated_menu_buttons_supported(cfg.three_key_navigation) {
            warn!(
                "only_dedicated_menu_buttons requires dedicated menu buttons for {} mode, but no player has the required bindings mapped — leaving gameplay button fallback enabled.",
                dedicated_menu_navigation_label(cfg.three_key_navigation)
            );
            false
        } else {
            enabled
        };
        if cfg.only_dedicated_menu_buttons == enabled {
            return;
        }
        cfg.only_dedicated_menu_buttons = enabled;
        enabled
    };
    deadsync_input::set_only_dedicated_menu_buttons(enabled);
    save_without_keymaps();
}

pub fn update_keyboard_features(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.keyboard_features);
}

pub fn update_visual_style(style: VisualStyle) {
    update_config_value(style, |cfg| &mut cfg.visual_style);
}

pub fn update_srpg_variant(variant: SrpgVariant) {
    update_config_value(variant, |cfg| &mut cfg.srpg_variant);
}

pub fn update_machine_show_select_profile(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_show_select_profile);
}

pub fn update_allow_switch_profile_in_menu(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.allow_switch_profile_in_menu);
}

pub fn update_show_video_backgrounds(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_video_backgrounds);
}

pub fn update_random_background_mode(mode: RandomBackgroundMode) {
    update_config_value(mode, |cfg| &mut cfg.random_background_mode);
}

pub fn update_write_current_screen(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.write_current_screen);
}

pub fn update_machine_show_select_color(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_show_select_color);
}

pub fn update_machine_show_select_style(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_show_select_style);
}

pub fn update_machine_show_select_play_mode(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_show_select_play_mode);
}

pub fn update_machine_preferred_style(style: MachinePreferredPlayStyle) {
    update_config_value(style, |cfg| &mut cfg.machine_preferred_style);
}

pub fn update_machine_preferred_play_mode(mode: MachinePreferredPlayMode) {
    update_config_value(mode, |cfg| &mut cfg.machine_preferred_play_mode);
}

pub fn update_machine_show_eval_summary(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_show_eval_summary);
}

pub fn update_machine_nice_sound(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_nice_sound);
}

pub fn update_machine_show_name_entry(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_show_name_entry);
}

pub fn update_machine_show_gameover(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_show_gameover);
}

pub fn update_machine_enable_replays(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_enable_replays);
}

pub fn update_machine_allow_per_player_global_offsets(enabled: bool) {
    update_config_value(enabled, |cfg| {
        &mut cfg.machine_allow_per_player_global_offsets
    });
}

pub fn update_machine_pack_ini_offsets(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.machine_pack_ini_offsets);
}

pub fn update_machine_default_sync_offset(offset: DefaultSyncOffset) {
    update_config_value(offset, |cfg| &mut cfg.machine_default_sync_offset);
}

pub fn update_enable_groovestats(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.enable_groovestats);
}

pub fn update_enable_boogiestats(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.enable_boogiestats);
}

pub fn update_enable_arrowcloud(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.enable_arrowcloud);
}

pub fn update_submit_arrowcloud_fails(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.submit_arrowcloud_fails);
}

pub fn update_arrowcloud_qr_login_when(when: ArrowCloudQrLoginWhen) {
    update_config_value(when, |cfg| &mut cfg.arrowcloud_qr_login_when);
}

pub fn update_groovestats_qr_login_when(when: GrooveStatsQrLoginWhen) {
    update_config_value(when, |cfg| &mut cfg.groovestats_qr_login_when);
}

pub fn update_auto_download_unlocks(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.auto_download_unlocks);
}

pub fn update_auto_populate_gs_scores(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.auto_populate_gs_scores);
}

pub fn update_separate_unlocks_by_player(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.separate_unlocks_by_player);
}

pub fn update_game_flag(flag: GameFlag) {
    update_config_value(flag, |cfg| &mut cfg.game_flag);
}

pub fn update_theme_flag(flag: ThemeFlag) {
    update_config_value(flag, |cfg| &mut cfg.theme_flag);
}

pub fn update_language_flag(flag: LanguageFlag) {
    update_config_value(flag, |cfg| &mut cfg.language_flag);
}

pub fn update_machine_default_noteskin(noteskin: &str) {
    let normalized = normalize_machine_default_noteskin(noteskin);
    {
        let mut current = MACHINE_DEFAULT_NOTESKIN.lock().unwrap();
        if *current == normalized {
            return;
        }
        *current = normalized;
    }
    save_without_keymaps();
}
