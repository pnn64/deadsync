use super::*;

#[inline(always)]
fn dedicated_menu_buttons_supported(three_key_navigation: bool) -> bool {
    crate::engine::input::any_player_has_dedicated_menu_buttons_for_mode(three_key_navigation)
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
    let seconds = seconds.clamp(0.0, 0.2);
    {
        let mut cfg = lock_config();
        if (cfg.input_debounce_seconds - seconds).abs() <= f32::EPSILON {
            return;
        }
        cfg.input_debounce_seconds = seconds;
    }
    crate::engine::input::set_input_debounce_seconds(seconds);
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
    crate::engine::input::set_only_dedicated_menu_buttons(dedicated);
    save_without_keymaps();
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
    crate::engine::input::set_only_dedicated_menu_buttons(enabled);
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

pub fn update_submit_groovestats_fails(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.submit_groovestats_fails == enabled {
            return;
        }
        cfg.submit_groovestats_fails = enabled;
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
