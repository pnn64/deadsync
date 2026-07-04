use super::*;
use deadsync_config::theme::{
    parse_machine_default_sync_offset, parse_machine_font, parse_srpg_variant, parse_visual_style,
};

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
    let visual_style = conf.get("Theme", "VisualStyle");
    let legacy_visual_style = conf.get("Theme", "MenuBackgroundStyle");
    cfg.visual_style = parse_visual_style(
        visual_style.as_deref(),
        legacy_visual_style.as_deref(),
        default.visual_style,
    );
    let srpg_variant = conf.get("Theme", "SrpgVariant");
    let legacy_srpg_variant = conf.get("Theme", "ThemeVariant");
    cfg.srpg_variant = parse_srpg_variant(
        srpg_variant.as_deref(),
        legacy_srpg_variant.as_deref(),
        visual_style.or(legacy_visual_style).as_deref(),
        default.srpg_variant,
    );
    cfg.show_video_backgrounds = conf
        .get("Theme", "VideoBackgrounds")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.show_video_backgrounds);
    cfg.random_background_mode = conf
        .get("Theme", "RandomBackgroundMode")
        .and_then(|v| RandomBackgroundMode::from_str(&v).ok())
        .unwrap_or(default.random_background_mode);
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
    cfg.machine_nice_sound = conf
        .get("Theme", "MachineNiceSound")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_nice_sound);
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
    cfg.music_select_shortcut_practice = conf
        .get("Theme", "SelectMusicShortcutPractice")
        .and_then(|v| parse_keycode_to_key(&v))
        .unwrap_or(default.music_select_shortcut_practice);
    cfg.music_select_shortcut_song_search = conf
        .get("Theme", "SelectMusicShortcutSongSearch")
        .and_then(|v| parse_keycode_to_key(&v))
        .unwrap_or(default.music_select_shortcut_song_search);
    cfg.music_select_shortcut_load_songs = conf
        .get("Theme", "SelectMusicShortcutLoadSongs")
        .and_then(|v| parse_keycode_to_key(&v))
        .unwrap_or(default.music_select_shortcut_load_songs);
    cfg.music_select_shortcut_test_input = conf
        .get("Theme", "SelectMusicShortcutTestInput")
        .and_then(|v| parse_keycode_to_key(&v))
        .unwrap_or(default.music_select_shortcut_test_input);
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
    cfg.machine_pack_ini_offsets = conf
        .get("Theme", "MachinePackIniOffsets")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.machine_pack_ini_offsets);
    let machine_default_sync_offset = conf.get("Theme", "MachineDefaultSyncOffset");
    let legacy_default_sync_offset = conf.get("Theme", "DefaultSyncOffset");
    cfg.machine_default_sync_offset = parse_machine_default_sync_offset(
        machine_default_sync_offset.as_deref(),
        legacy_default_sync_offset.as_deref(),
        default.machine_default_sync_offset,
    );
    cfg.machine_preferred_style = conf
        .get("Theme", "MachinePreferredStyle")
        .and_then(|v| MachinePreferredPlayStyle::from_str(&v).ok())
        .unwrap_or(default.machine_preferred_style);
    cfg.machine_preferred_play_mode = conf
        .get("Theme", "MachinePreferredPlayMode")
        .and_then(|v| MachinePreferredPlayMode::from_str(&v).ok())
        .unwrap_or(default.machine_preferred_play_mode);
    let machine_font = conf.get("Theme", "MachineFont");
    let legacy_machine_font = conf.get("Theme", "ThemeFont");
    cfg.machine_font = parse_machine_font(
        machine_font.as_deref(),
        legacy_machine_font.as_deref(),
        default.machine_font,
    );
    cfg.machine_bar_color = conf
        .get("Theme", "MachineBarColor")
        .and_then(|v| MachineBarColor::from_str(&v).ok())
        .unwrap_or(default.machine_bar_color);
    cfg.machine_evaluation_style = conf
        .get("Theme", "MachineEvaluationStyle")
        .and_then(|v| MachineEvaluationStyle::from_str(&v).ok())
        .unwrap_or(default.machine_evaluation_style);
}
