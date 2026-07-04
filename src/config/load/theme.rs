use super::*;
use deadsync_config::theme::{
    MachineFlowOptions, ThemePresentationOptions, load_machine_flow_options,
    load_theme_presentation_options,
};

pub(super) fn load(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    load_theme_presentation(conf, default, cfg);
    load_machine_flow(conf, default, cfg);
}

fn load_theme_presentation(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    let loaded = load_theme_presentation_options(
        conf,
        ThemePresentationOptions {
            simply_love_color: default.simply_love_color,
            show_select_music_gameplay_timer: default.show_select_music_gameplay_timer,
            keyboard_features: default.keyboard_features,
            visual_style: default.visual_style,
            srpg_variant: default.srpg_variant,
            show_video_backgrounds: default.show_video_backgrounds,
            random_background_mode: default.random_background_mode,
            zmod_rating_box_text: default.zmod_rating_box_text,
            show_bpm_decimal: default.show_bpm_decimal,
            gameplay_bpm_position: default.gameplay_bpm_position,
        },
    );
    cfg.simply_love_color = loaded.simply_love_color;
    cfg.show_select_music_gameplay_timer = loaded.show_select_music_gameplay_timer;
    cfg.keyboard_features = loaded.keyboard_features;
    cfg.visual_style = loaded.visual_style;
    cfg.srpg_variant = loaded.srpg_variant;
    cfg.show_video_backgrounds = loaded.show_video_backgrounds;
    cfg.random_background_mode = loaded.random_background_mode;
    cfg.zmod_rating_box_text = loaded.zmod_rating_box_text;
    cfg.show_bpm_decimal = loaded.show_bpm_decimal;
    cfg.gameplay_bpm_position = loaded.gameplay_bpm_position;
}

fn load_machine_flow(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    let loaded = load_machine_flow_options(
        conf,
        MachineFlowOptions {
            machine_show_eval_summary: default.machine_show_eval_summary,
            machine_nice_sound: default.machine_nice_sound,
            machine_show_name_entry: default.machine_show_name_entry,
            machine_show_gameover: default.machine_show_gameover,
            machine_show_select_profile: default.machine_show_select_profile,
            allow_switch_profile_in_menu: default.allow_switch_profile_in_menu,
            machine_show_select_color: default.machine_show_select_color,
            machine_show_select_style: default.machine_show_select_style,
            machine_show_select_play_mode: default.machine_show_select_play_mode,
            machine_enable_replays: default.machine_enable_replays,
            machine_allow_per_player_global_offsets: default
                .machine_allow_per_player_global_offsets,
            machine_pack_ini_offsets: default.machine_pack_ini_offsets,
            machine_default_sync_offset: default.machine_default_sync_offset,
            machine_preferred_style: default.machine_preferred_style,
            machine_preferred_play_mode: default.machine_preferred_play_mode,
            machine_font: default.machine_font,
            machine_bar_color: default.machine_bar_color,
            machine_evaluation_style: default.machine_evaluation_style,
        },
    );
    cfg.machine_show_eval_summary = loaded.machine_show_eval_summary;
    cfg.machine_nice_sound = loaded.machine_nice_sound;
    cfg.machine_show_name_entry = loaded.machine_show_name_entry;
    cfg.machine_show_gameover = loaded.machine_show_gameover;
    cfg.machine_show_select_profile = loaded.machine_show_select_profile;
    cfg.allow_switch_profile_in_menu = loaded.allow_switch_profile_in_menu;
    cfg.machine_show_select_color = loaded.machine_show_select_color;
    cfg.machine_show_select_style = loaded.machine_show_select_style;
    cfg.machine_show_select_play_mode = loaded.machine_show_select_play_mode;
    cfg.machine_enable_replays = loaded.machine_enable_replays;
    cfg.machine_allow_per_player_global_offsets = loaded.machine_allow_per_player_global_offsets;
    cfg.machine_pack_ini_offsets = loaded.machine_pack_ini_offsets;
    cfg.machine_default_sync_offset = loaded.machine_default_sync_offset;
    cfg.machine_preferred_style = loaded.machine_preferred_style;
    cfg.machine_preferred_play_mode = loaded.machine_preferred_play_mode;
    cfg.machine_font = loaded.machine_font;
    cfg.machine_bar_color = loaded.machine_bar_color;
    cfg.machine_evaluation_style = loaded.machine_evaluation_style;

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
}
