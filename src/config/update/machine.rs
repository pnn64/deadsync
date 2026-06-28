use super::*;

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
    {
        let mut cfg = lock_config();
        if (cfg.input_debounce_seconds - seconds).abs() <= f32::EPSILON {
            return;
        }
        cfg.input_debounce_seconds = seconds;
    }
    deadsync_input::set_input_debounce_seconds(seconds);
    save_without_keymaps();
}

pub fn update_arcade_options_navigation(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.arcade_options_navigation == enabled {
            return;
        }
        cfg.arcade_options_navigation = enabled;
    }
    save_without_keymaps();
}

pub fn update_delayed_back(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.delayed_back == enabled {
            return;
        }
        cfg.delayed_back = enabled;
    }
    save_without_keymaps();
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
    {
        let mut cfg = lock_config();
        if cfg.use_fsrs == enabled {
            return;
        }
        cfg.use_fsrs = enabled;
    }
    save_without_keymaps();
}

/// Persist the StepManiaX-pad input toggle. The SMX manager and listeners are
/// wired at startup, so this takes effect on the next launch (mirroring the
/// gamepad-backend option).
pub fn update_smx_input(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.smx_input == enabled {
            return;
        }
        cfg.smx_input = enabled;
    }
    save_without_keymaps();
}

/// Persist the "DeadSync manages pad config" toggle. When on, the app loop
/// (`apply_smx_managed_preset`) resolves and writes each connected SMX pad's
/// config every non-gameplay frame; this just records the preference.
pub fn update_smx_manages_pad_config(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.smx_manages_pad_config == enabled {
            return;
        }
        cfg.smx_manages_pad_config = enabled;
    }
    save_without_keymaps();
}

/// Persist whether SMX pad panels light up during gameplay. Saving the flag is all this
/// needs to do: `App::sync_lights` reads it from config alongside the other lights settings
/// and activates or releases the panel worker accordingly, so there is no separate driver or
/// runtime state to update here.
pub fn update_smx_panel_lights(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.smx_panel_lights == enabled {
            return;
        }
        cfg.smx_panel_lights = enabled;
    }
    save_without_keymaps();
}

/// Persist the built-in default pad preset (Low/Medium/High). Used as the
/// fallback config flashed to a managed pad when no saved config resolves.
pub fn update_smx_default_pad_config(preset: crate::config::SmxPadPreset) {
    {
        let mut cfg = lock_config();
        if cfg.smx_default_pad_config == preset {
            return;
        }
        cfg.smx_default_pad_config = preset;
    }
    save_without_keymaps();
}

/// Persist the machine-default pad-light brightness (0..=100). This seeds new
/// player profiles; it does not retroactively change existing profiles, so there
/// is nothing to push to the SDK here (the per-slot live value is resolved from
/// the active profiles by `App::sync_smx_light_brightness`).
pub fn update_smx_default_light_brightness(percent: u8) {
    let percent = percent.min(100);
    {
        let mut cfg = lock_config();
        if cfg.smx_default_light_brightness == percent {
            return;
        }
        cfg.smx_default_light_brightness = percent;
    }
    save_without_keymaps();
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
    {
        let mut cfg = lock_config();
        if cfg.keyboard_features == enabled {
            return;
        }
        cfg.keyboard_features = enabled;
    }
    save_without_keymaps();
}

pub fn update_visual_style(style: VisualStyle) {
    {
        let mut cfg = lock_config();
        if cfg.visual_style == style {
            return;
        }
        cfg.visual_style = style;
    }
    save_without_keymaps();
}

pub fn update_srpg_variant(variant: SrpgVariant) {
    {
        let mut cfg = lock_config();
        if cfg.srpg_variant == variant {
            return;
        }
        cfg.srpg_variant = variant;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_profile(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_profile == enabled {
            return;
        }
        cfg.machine_show_select_profile = enabled;
    }
    save_without_keymaps();
}

pub fn update_allow_switch_profile_in_menu(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.allow_switch_profile_in_menu == enabled {
            return;
        }
        cfg.allow_switch_profile_in_menu = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_video_backgrounds(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_video_backgrounds == enabled {
            return;
        }
        cfg.show_video_backgrounds = enabled;
    }
    save_without_keymaps();
}

pub fn update_random_background_mode(mode: RandomBackgroundMode) {
    {
        let mut cfg = lock_config();
        if cfg.random_background_mode == mode {
            return;
        }
        cfg.random_background_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_write_current_screen(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.write_current_screen == enabled {
            return;
        }
        cfg.write_current_screen = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_color(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_color == enabled {
            return;
        }
        cfg.machine_show_select_color = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_style(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_style == enabled {
            return;
        }
        cfg.machine_show_select_style = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_play_mode(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_play_mode == enabled {
            return;
        }
        cfg.machine_show_select_play_mode = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_preferred_style(style: MachinePreferredPlayStyle) {
    {
        let mut cfg = lock_config();
        if cfg.machine_preferred_style == style {
            return;
        }
        cfg.machine_preferred_style = style;
    }
    save_without_keymaps();
}

pub fn update_machine_preferred_play_mode(mode: MachinePreferredPlayMode) {
    {
        let mut cfg = lock_config();
        if cfg.machine_preferred_play_mode == mode {
            return;
        }
        cfg.machine_preferred_play_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_machine_show_eval_summary(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_eval_summary == enabled {
            return;
        }
        cfg.machine_show_eval_summary = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_nice_sound(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_nice_sound == enabled {
            return;
        }
        cfg.machine_nice_sound = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_name_entry(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_name_entry == enabled {
            return;
        }
        cfg.machine_show_name_entry = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_gameover(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_gameover == enabled {
            return;
        }
        cfg.machine_show_gameover = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_enable_replays(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_enable_replays == enabled {
            return;
        }
        cfg.machine_enable_replays = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_allow_per_player_global_offsets(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_allow_per_player_global_offsets == enabled {
            return;
        }
        cfg.machine_allow_per_player_global_offsets = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_pack_ini_offsets(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_pack_ini_offsets == enabled {
            return;
        }
        cfg.machine_pack_ini_offsets = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_default_sync_offset(offset: DefaultSyncOffset) {
    {
        let mut cfg = lock_config();
        if cfg.machine_default_sync_offset == offset {
            return;
        }
        cfg.machine_default_sync_offset = offset;
    }
    save_without_keymaps();
}

pub fn update_enable_groovestats(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_groovestats == enabled {
            return;
        }
        cfg.enable_groovestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_boogiestats(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_boogiestats == enabled {
            return;
        }
        cfg.enable_boogiestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_arrowcloud(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_arrowcloud == enabled {
            return;
        }
        cfg.enable_arrowcloud = enabled;
    }
    save_without_keymaps();
}

pub fn update_submit_arrowcloud_fails(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.submit_arrowcloud_fails == enabled {
            return;
        }
        cfg.submit_arrowcloud_fails = enabled;
    }
    save_without_keymaps();
}

pub fn update_arrowcloud_qr_login_when(when: ArrowCloudQrLoginWhen) {
    {
        let mut cfg = lock_config();
        if cfg.arrowcloud_qr_login_when == when {
            return;
        }
        cfg.arrowcloud_qr_login_when = when;
    }
    save_without_keymaps();
}

pub fn update_groovestats_qr_login_when(when: GrooveStatsQrLoginWhen) {
    {
        let mut cfg = lock_config();
        if cfg.groovestats_qr_login_when == when {
            return;
        }
        cfg.groovestats_qr_login_when = when;
    }
    save_without_keymaps();
}

pub fn update_auto_download_unlocks(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.auto_download_unlocks == enabled {
            return;
        }
        cfg.auto_download_unlocks = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_populate_gs_scores(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.auto_populate_gs_scores == enabled {
            return;
        }
        cfg.auto_populate_gs_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_separate_unlocks_by_player(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.separate_unlocks_by_player == enabled {
            return;
        }
        cfg.separate_unlocks_by_player = enabled;
    }
    save_without_keymaps();
}

pub fn update_game_flag(flag: GameFlag) {
    {
        let mut cfg = lock_config();
        if cfg.game_flag == flag {
            return;
        }
        cfg.game_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_theme_flag(flag: ThemeFlag) {
    {
        let mut cfg = lock_config();
        if cfg.theme_flag == flag {
            return;
        }
        cfg.theme_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_language_flag(flag: LanguageFlag) {
    {
        let mut cfg = lock_config();
        if cfg.language_flag == flag {
            return;
        }
        cfg.language_flag = flag;
    }
    save_without_keymaps();
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
