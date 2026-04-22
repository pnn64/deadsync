use super::*;

pub(super) fn load(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    load_theme_presentation(conf, default, cfg);
    load_machine_flow(conf, default, cfg);
}

fn load_theme_presentation(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.simply_love_color = conf
        .get("Theme", "SimplyLoveColor")
        .and_then(|v| v.parse().ok())
        .unwrap_or(default.simply_love_color);
    cfg.show_select_music_gameplay_timer = conf
        .get("Theme", "ShowSelectMusicGameplayTimer")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.show_select_music_gameplay_timer);
    cfg.keyboard_features = conf
        .get("Theme", "KeyboardFeatures")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.keyboard_features);
    cfg.show_video_backgrounds = conf
        .get("Theme", "VideoBackgrounds")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.show_video_backgrounds);
    cfg.zmod_rating_box_text = conf
        .get("Theme", "ZmodRatingBoxText")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.zmod_rating_box_text);
    cfg.show_bpm_decimal = conf
        .get("Theme", "ShowBpmDecimal")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.show_bpm_decimal);
}

fn load_machine_flow(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.machine_show_eval_summary = conf
        .get("Theme", "MachineShowEvalSummary")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.machine_show_eval_summary);
    cfg.machine_show_name_entry = conf
        .get("Theme", "MachineShowNameEntry")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_show_name_entry);
    cfg.machine_show_gameover = conf
        .get("Theme", "MachineShowGameOver")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_show_gameover);
    cfg.machine_show_select_profile = conf
        .get("Theme", "MachineShowSelectProfile")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_show_select_profile);
    cfg.allow_switch_profile_in_menu = conf
        .get("Theme", "AllowSwitchProfileInMenu")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.allow_switch_profile_in_menu);
    cfg.machine_show_select_color = conf
        .get("Theme", "MachineShowSelectColor")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_show_select_color);
    cfg.machine_show_select_style = conf
        .get("Theme", "MachineShowSelectStyle")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_show_select_style);
    cfg.machine_show_select_play_mode = conf
        .get("Theme", "MachineShowSelectPlayMode")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_show_select_play_mode);
    cfg.machine_enable_replays = conf
        .get("Theme", "MachineEnableReplays")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_enable_replays);
    cfg.machine_allow_per_player_global_offsets = conf
        .get("Theme", "MachineAllowPerPlayerGlobalOffsets")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_allow_per_player_global_offsets);
    cfg.machine_preferred_style = conf
        .get("Theme", "MachinePreferredStyle")
        .and_then(|v| MachinePreferredPlayStyle::from_str(&v).ok())
        .unwrap_or(default.machine_preferred_style);
    cfg.machine_preferred_play_mode = conf
        .get("Theme", "MachinePreferredPlayMode")
        .and_then(|v| MachinePreferredPlayMode::from_str(&v).ok())
        .unwrap_or(default.machine_preferred_play_mode);
    cfg.machine_font = conf
        .get("Theme", "MachineFont")
        .or_else(|| conf.get("Theme", "ThemeFont"))
        .and_then(|v| MachineFont::from_str(&v).ok())
        .unwrap_or(default.machine_font);
}
