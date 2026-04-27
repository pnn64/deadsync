use super::{
    AccelEffectsMask, AppearanceEffectsMask, AttackMode, BackgroundFilter, ComboColors, ComboFont,
    ComboMode, DataVisualizations, ErrorBarMask, ErrorBarTrim, HUD_OFFSET_MAX, HUD_OFFSET_MIN,
    HideLightType, HoldJudgmentGraphic, HoldsMask, InsertMask, JudgmentGraphic, LifeMeterType,
    MeasureCounter, MeasureLines, MiniIndicator, MiniIndicatorScoreType, NoteSkin, Perspective,
    PlayStyle, PlayerSide, RemoveMask, SPACING_PERCENT_MAX, SPACING_PERCENT_MIN, ScrollOption,
    ScrollSpeedSetting, TargetScoreSetting, TimingWindowsOption, TurnOption, VisualEffectsMask,
    clamp_custom_fantastic_window_ms, error_bar_style_from_mask, error_bar_text_from_mask,
    lock_profiles, sanitize_player_initials, save_profile_ini_for_side,
    save_profile_stats_for_side, session_side_is_guest, side_ix,
};
use chrono::Local;
use std::path::Path;

pub fn update_last_played_for_side(
    side: PlayerSide,
    style: PlayStyle,
    music_path: Option<&Path>,
    chart_hash: Option<&str>,
    difficulty_index: usize,
) {
    if session_side_is_guest(side) {
        return;
    }
    let new_path = music_path.map(|p| p.to_string_lossy().into_owned());
    let new_hash = chart_hash.map(str::to_string);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        let last_played = profile.last_played_mut(style);
        let mut changed = false;
        if last_played.song_music_path != new_path {
            last_played.song_music_path = new_path;
            changed = true;
        }
        if last_played.chart_hash != new_hash {
            last_played.chart_hash = new_hash;
            changed = true;
        }
        if last_played.difficulty_index != difficulty_index {
            last_played.difficulty_index = difficulty_index;
            changed = true;
        }
        if !changed {
            return;
        }
    }
    save_profile_ini_for_side(side);
}

pub fn add_stage_calories_for_side(side: PlayerSide, calories_burned: f32) {
    if session_side_is_guest(side) {
        return;
    }

    let today = Local::now().date_naive().to_string();
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];

        if profile.calories_burned_day.trim() != today {
            profile.calories_burned_day.clone_from(&today);
            profile.calories_burned_today = 0.0;
        }

        if !profile.ignore_step_count_calories
            && calories_burned.is_finite()
            && calories_burned >= 0.0
        {
            profile.calories_burned_today =
                (profile.calories_burned_today + calories_burned).max(0.0);
        }
    }
    save_profile_ini_for_side(side);
}

pub fn update_player_initials_for_side(side: PlayerSide, initials: &str) {
    if session_side_is_guest(side) {
        return;
    }
    let initials = sanitize_player_initials(initials);
    if initials.is_empty() {
        return;
    }
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.player_initials == initials {
            return;
        }
        profile.player_initials = initials;
    }
    save_profile_ini_for_side(side);
}

pub fn update_scroll_speed_for_side(side: PlayerSide, setting: ScrollSpeedSetting) {
    // Guest changes should persist for the active session; save_* no-ops for guests.
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.scroll_speed == setting {
            return;
        }
        profile.scroll_speed = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_background_filter_for_side(side: PlayerSide, value: i32) {
    let new = BackgroundFilter::from_i32(value);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.background_filter == new {
            return;
        }
        profile.background_filter = new;
    }
    save_profile_ini_for_side(side);
}

pub fn update_hold_judgment_graphic_for_side(side: PlayerSide, setting: HoldJudgmentGraphic) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.hold_judgment_graphic == setting {
            return;
        }
        profile.hold_judgment_graphic = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_judgment_graphic_for_side(side: PlayerSide, setting: JudgmentGraphic) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.judgment_graphic == setting {
            return;
        }
        profile.judgment_graphic = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_combo_font_for_side(side: PlayerSide, setting: ComboFont) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_font == setting {
            return;
        }
        profile.combo_font = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_combo_colors_for_side(side: PlayerSide, setting: ComboColors) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_colors == setting {
            return;
        }
        profile.combo_colors = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_combo_mode_for_side(side: PlayerSide, setting: ComboMode) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_mode == setting {
            return;
        }
        profile.combo_mode = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_carry_combo_between_songs_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.carry_combo_between_songs == enabled {
            return;
        }
        profile.carry_combo_between_songs = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_current_combo_for_side(side: PlayerSide, combo: u32) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.current_combo == combo {
            return;
        }
        profile.current_combo = combo;
    }
    save_profile_stats_for_side(side);
}

pub fn update_scroll_option_for_side(side: PlayerSide, setting: ScrollOption) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        let reverse_enabled = setting.contains(ScrollOption::Reverse);
        if profile.scroll_option == setting && profile.reverse_scroll == reverse_enabled {
            return;
        }
        profile.scroll_option = setting;
        profile.reverse_scroll = reverse_enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_turn_option_for_side(side: PlayerSide, setting: TurnOption) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.turn_option == setting {
            return;
        }
        profile.turn_option = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_insert_mask_for_side(side: PlayerSide, mask: InsertMask) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.insert_active_mask == mask {
            return;
        }
        profile.insert_active_mask = mask;
    }
    save_profile_ini_for_side(side);
}

pub fn update_remove_mask_for_side(side: PlayerSide, mask: RemoveMask) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.remove_active_mask == mask {
            return;
        }
        profile.remove_active_mask = mask;
    }
    save_profile_ini_for_side(side);
}

pub fn update_holds_mask_for_side(side: PlayerSide, mask: HoldsMask) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.holds_active_mask == mask {
            return;
        }
        profile.holds_active_mask = mask;
    }
    save_profile_ini_for_side(side);
}

pub fn update_accel_effects_mask_for_side(side: PlayerSide, mask: AccelEffectsMask) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.accel_effects_active_mask == mask {
            return;
        }
        profile.accel_effects_active_mask = mask;
    }
    save_profile_ini_for_side(side);
}

pub fn update_visual_effects_mask_for_side(side: PlayerSide, mask: VisualEffectsMask) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.visual_effects_active_mask == mask {
            return;
        }
        profile.visual_effects_active_mask = mask;
    }
    save_profile_ini_for_side(side);
}

pub fn update_appearance_effects_mask_for_side(side: PlayerSide, mask: AppearanceEffectsMask) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.appearance_effects_active_mask == mask {
            return;
        }
        profile.appearance_effects_active_mask = mask;
    }
    save_profile_ini_for_side(side);
}

pub fn update_attack_mode_for_side(side: PlayerSide, setting: AttackMode) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.attack_mode == setting {
            return;
        }
        profile.attack_mode = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_hide_light_type_for_side(side: PlayerSide, setting: HideLightType) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.hide_light_type == setting {
            return;
        }
        profile.hide_light_type = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_rescore_early_hits_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.rescore_early_hits == enabled {
            return;
        }
        profile.rescore_early_hits = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_early_dw_options_for_side(side: PlayerSide, hide_judgments: bool, hide_flash: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.hide_early_dw_judgments == hide_judgments
            && profile.hide_early_dw_flash == hide_flash
        {
            return;
        }
        profile.hide_early_dw_judgments = hide_judgments;
        profile.hide_early_dw_flash = hide_flash;
    }
    save_profile_ini_for_side(side);
}

pub fn update_timing_windows_for_side(side: PlayerSide, setting: TimingWindowsOption) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.timing_windows == setting {
            return;
        }
        profile.timing_windows = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_hide_options_for_side(
    side: PlayerSide,
    hide_targets: bool,
    hide_song_bg: bool,
    hide_combo: bool,
    hide_lifebar: bool,
    hide_score: bool,
    hide_danger: bool,
    hide_combo_explosions: bool,
) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.hide_targets == hide_targets
            && profile.hide_song_bg == hide_song_bg
            && profile.hide_combo == hide_combo
            && profile.hide_lifebar == hide_lifebar
            && profile.hide_score == hide_score
            && profile.hide_danger == hide_danger
            && profile.hide_combo_explosions == hide_combo_explosions
        {
            return;
        }
        profile.hide_targets = hide_targets;
        profile.hide_song_bg = hide_song_bg;
        profile.hide_combo = hide_combo;
        profile.hide_lifebar = hide_lifebar;
        profile.hide_score = hide_score;
        profile.hide_danger = hide_danger;
        profile.hide_combo_explosions = hide_combo_explosions;
    }
    save_profile_ini_for_side(side);
}

pub fn update_gameplay_extras_for_side(
    side: PlayerSide,
    column_flash_on_miss: bool,
    subtractive_scoring: bool,
    pacemaker: bool,
    nps_graph_at_top: bool,
) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.column_flash_on_miss == column_flash_on_miss
            && profile.subtractive_scoring == subtractive_scoring
            && profile.pacemaker == pacemaker
            && profile.nps_graph_at_top == nps_graph_at_top
        {
            return;
        }
        profile.column_flash_on_miss = column_flash_on_miss;
        profile.subtractive_scoring = subtractive_scoring;
        profile.pacemaker = pacemaker;
        profile.nps_graph_at_top = nps_graph_at_top;
        if subtractive_scoring {
            profile.mini_indicator = MiniIndicator::SubtractiveScoring;
        } else if pacemaker {
            profile.mini_indicator = MiniIndicator::Pacemaker;
        } else if matches!(
            profile.mini_indicator,
            MiniIndicator::SubtractiveScoring | MiniIndicator::Pacemaker
        ) {
            profile.mini_indicator = MiniIndicator::None;
        }
    }
    save_profile_ini_for_side(side);
}

pub fn update_transparent_density_graph_bg_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.transparent_density_graph_bg == enabled {
            return;
        }
        profile.transparent_density_graph_bg = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_mini_indicator_for_side(side: PlayerSide, setting: MiniIndicator) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.mini_indicator == setting {
            return;
        }
        profile.mini_indicator = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_mini_indicator_score_type_for_side(
    side: PlayerSide,
    setting: MiniIndicatorScoreType,
) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.mini_indicator_score_type == setting {
            return;
        }
        profile.mini_indicator_score_type = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_noteskin_for_side(side: PlayerSide, setting: NoteSkin) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.noteskin == setting {
            return;
        }
        profile.noteskin = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_mine_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.mine_noteskin == setting {
            return;
        }
        profile.mine_noteskin = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_receptor_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.receptor_noteskin == setting {
            return;
        }
        profile.receptor_noteskin = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_tap_explosion_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.tap_explosion_noteskin == setting {
            return;
        }
        profile.tap_explosion_noteskin = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_notefield_offset_x_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(0, 50);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.note_field_offset_x == clamped {
            return;
        }
        profile.note_field_offset_x = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_notefield_offset_y_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(-50, 50);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.note_field_offset_y == clamped {
            return;
        }
        profile.note_field_offset_y = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_judgment_offset_x_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.judgment_offset_x == clamped {
            return;
        }
        profile.judgment_offset_x = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_judgment_offset_y_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.judgment_offset_y == clamped {
            return;
        }
        profile.judgment_offset_y = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_combo_offset_x_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_offset_x == clamped {
            return;
        }
        profile.combo_offset_x = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_combo_offset_y_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.combo_offset_y == clamped {
            return;
        }
        profile.combo_offset_y = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_offset_x_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_offset_x == clamped {
            return;
        }
        profile.error_bar_offset_x = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_offset_y_for_side(side: PlayerSide, offset: i32) {
    let clamped = offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_offset_y == clamped {
            return;
        }
        profile.error_bar_offset_y = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_mini_percent_for_side(side: PlayerSide, percent: i32) {
    // Mirror Simply Love's range: -100% to +150%.
    let clamped = percent.clamp(-100, 150);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.mini_percent == clamped {
            return;
        }
        profile.mini_percent = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_spacing_percent_for_side(side: PlayerSide, percent: i32) {
    // Mirror zmod's range: -100% to +100%, step 1.
    let clamped = percent.clamp(SPACING_PERCENT_MIN, SPACING_PERCENT_MAX);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.spacing_percent == clamped {
            return;
        }
        profile.spacing_percent = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_perspective_for_side(side: PlayerSide, perspective: Perspective) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.perspective == perspective {
            return;
        }
        profile.perspective = perspective;
    }
    save_profile_ini_for_side(side);
}

pub fn update_visual_delay_ms_for_side(side: PlayerSide, ms: i32) {
    // Mirror Simply Love's range: -100ms to +100ms.
    let clamped = ms.clamp(-100, 100);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.visual_delay_ms == clamped {
            return;
        }
        profile.visual_delay_ms = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_global_offset_shift_ms_for_side(side: PlayerSide, ms: i32) {
    // Keep the personal timing shift in the same small-calibration range as visual delay.
    let clamped = ms.clamp(-100, 100);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.global_offset_shift_ms == clamped {
            return;
        }
        profile.global_offset_shift_ms = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_fa_plus_window_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_fa_plus_window == enabled {
            return;
        }
        profile.show_fa_plus_window = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_ex_score_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_ex_score == enabled {
            return;
        }
        profile.show_ex_score = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_hard_ex_score_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_hard_ex_score == enabled {
            return;
        }
        profile.show_hard_ex_score = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_fa_plus_pane_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_fa_plus_pane == enabled {
            return;
        }
        profile.show_fa_plus_pane = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_fa_plus_10ms_blue_window_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.fa_plus_10ms_blue_window == enabled {
            return;
        }
        profile.fa_plus_10ms_blue_window = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_track_early_judgments_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.track_early_judgments == enabled {
            return;
        }
        profile.track_early_judgments = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_split_15_10ms_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.split_15_10ms == enabled {
            return;
        }
        profile.split_15_10ms = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_custom_fantastic_window_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.custom_fantastic_window == enabled {
            return;
        }
        profile.custom_fantastic_window = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_custom_fantastic_window_ms_for_side(side: PlayerSide, ms: u8) {
    let clamped = clamp_custom_fantastic_window_ms(ms);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.custom_fantastic_window_ms == clamped {
            return;
        }
        profile.custom_fantastic_window_ms = clamped;
    }
    save_profile_ini_for_side(side);
}

pub fn update_judgment_tilt_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.judgment_tilt == enabled {
            return;
        }
        profile.judgment_tilt = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_column_cues_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.column_cues == enabled {
            return;
        }
        profile.column_cues = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_judgment_back_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.judgment_back == enabled {
            return;
        }
        profile.judgment_back = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_ms_display_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_ms_display == enabled {
            return;
        }
        profile.error_ms_display = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_display_scorebox_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.display_scorebox == enabled {
            return;
        }
        profile.display_scorebox = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_rainbow_max_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.rainbow_max == enabled {
            return;
        }
        profile.rainbow_max = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_responsive_colors_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.responsive_colors == enabled {
            return;
        }
        profile.responsive_colors = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_show_life_percent_for_side(side: PlayerSide, enabled: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.show_life_percent == enabled {
            return;
        }
        profile.show_life_percent = enabled;
    }
    save_profile_ini_for_side(side);
}

pub fn update_tilt_multiplier_for_side(side: PlayerSide, multiplier: f32) {
    if !multiplier.is_finite() {
        return;
    }
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if (profile.tilt_multiplier - multiplier).abs() < 1e-6 {
            return;
        }
        profile.tilt_multiplier = multiplier;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_mask_for_side(side: PlayerSide, mask: ErrorBarMask) {
    let style = error_bar_style_from_mask(mask);
    let text = error_bar_text_from_mask(mask);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_active_mask == mask {
            return;
        }
        profile.error_bar_active_mask = mask;
        profile.error_bar = style;
        profile.error_bar_text = text;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_trim_for_side(side: PlayerSide, setting: ErrorBarTrim) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_trim == setting {
            return;
        }
        profile.error_bar_trim = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_data_visualizations_for_side(side: PlayerSide, setting: DataVisualizations) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.data_visualizations == setting {
            return;
        }
        profile.data_visualizations = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_target_score_for_side(side: PlayerSide, setting: TargetScoreSetting) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.target_score == setting {
            return;
        }
        profile.target_score = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_lifemeter_type_for_side(side: PlayerSide, setting: LifeMeterType) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.lifemeter_type == setting {
            return;
        }
        profile.lifemeter_type = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_error_bar_options_for_side(side: PlayerSide, up: bool, multi_tick: bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.error_bar_up == up && profile.error_bar_multi_tick == multi_tick {
            return;
        }
        profile.error_bar_up = up;
        profile.error_bar_multi_tick = multi_tick;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_counter_for_side(side: PlayerSide, setting: MeasureCounter) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_counter == setting {
            return;
        }
        profile.measure_counter = setting;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_counter_lookahead_for_side(side: PlayerSide, lookahead: u8) {
    let lookahead = lookahead.min(4);
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_counter_lookahead == lookahead {
            return;
        }
        profile.measure_counter_lookahead = lookahead;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_counter_options_for_side(
    side: PlayerSide,
    left: bool,
    up: bool,
    vert: bool,
    broken_run: bool,
    run_timer: bool,
) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_counter_left == left
            && profile.measure_counter_up == up
            && profile.measure_counter_vert == vert
            && profile.broken_run == broken_run
            && profile.run_timer == run_timer
        {
            return;
        }
        profile.measure_counter_left = left;
        profile.measure_counter_up = up;
        profile.measure_counter_vert = vert;
        profile.broken_run = broken_run;
        profile.run_timer = run_timer;
    }
    save_profile_ini_for_side(side);
}

pub fn update_measure_lines_for_side(side: PlayerSide, setting: MeasureLines) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.measure_lines == setting {
            return;
        }
        profile.measure_lines = setting;
    }
    save_profile_ini_for_side(side);
}
