// Frozen from d70f069f27c8fc853a8d8887a026942c7f6278a4.
use super::*;

use deadsync_config::ini::SimpleIni;
use std::path::Path;

#[derive(Debug, Default)]
pub struct OriginalProfileIni {
    sections: SimpleIni,
}

impl OriginalProfileIni {
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let mut sections = SimpleIni::new();
        sections.load(path)?;
        Ok(Self { sections })
    }

    pub fn parse(content: &str) -> Self {
        let mut sections = SimpleIni::new();
        sections.load_str(content);
        Self { sections }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections.get(section, key).map(str::to_owned)
    }

    pub fn section_has_any(&self, section: &str) -> bool {
        self.sections
            .get_section(section)
            .is_some_and(|s| !s.is_empty())
    }
}

pub fn original_runtime_save_profile_ini_for_side(
    root: &Path,
    side: PlayerSide,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    let profile_id = runtime_active_local_profile_id_for_side(side)?;
    let play_style = runtime_session_play_style();
    let profile = {
        let mut profiles = runtime_lock_profiles();
        let profile = &mut profiles[player_side_index(side)];
        profile.store_current_player_options(play_style);
        profile.clone()
    };
    let dir = runtime_profile_dir_for_profile_id(root, &profile_id, duplicate);
    write_profile_ini_dir(&dir, &profile_id, &profile)
        .err()
        .map(|error| RuntimeProfileSidecarWriteError {
            path: profile_ini_path(&dir),
            error,
        })
}

fn original_runtime_active_profile_id_and_data_for_side(
    side: PlayerSide,
) -> Option<(String, Profile)> {
    let profile_id = runtime_active_local_profile_id_for_side(side)?;
    let profile = runtime_lock_profiles()[player_side_index(side)].clone();
    Some((profile_id, profile))
}

pub fn original_runtime_save_groovestats_credentials_for_side(
    root: &Path,
    side: PlayerSide,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    let (profile_id, profile) = original_runtime_active_profile_id_and_data_for_side(side)?;
    let dir = runtime_profile_dir_for_profile_id(root, &profile_id, duplicate);
    write_groovestats_credentials_dir(
        &dir,
        &profile.groovestats_api_key,
        profile.groovestats_is_pad_player,
        &profile.groovestats_username,
        profile.groovestats_password.expose(),
    )
    .err()
    .map(|error| RuntimeProfileSidecarWriteError {
        path: groovestats_ini_path(&dir),
        error,
    })
}

pub fn original_runtime_save_arrowcloud_api_key_for_side(
    root: &Path,
    side: PlayerSide,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    let (profile_id, profile) = original_runtime_active_profile_id_and_data_for_side(side)?;
    let dir = runtime_profile_dir_for_profile_id(root, &profile_id, duplicate);
    write_arrowcloud_api_key_dir(&dir, &profile.arrowcloud_api_key)
        .err()
        .map(|error| RuntimeProfileSidecarWriteError {
            path: arrowcloud_ini_path(&dir),
            error,
        })
}

pub fn original_machine_player_defaults_from_ini(
    content: &str,
    base_common: &PlayerOptionsData,
) -> MachinePlayerDefaults {
    let ini = OriginalProfileIni::parse(content);
    let common = load_player_options_section(
        ini.section_has_any(COMMON_PLAYER_OPTIONS_SECTION),
        |key| ini.get(COMMON_PLAYER_OPTIONS_SECTION, key),
        base_common,
    )
    .unwrap_or_else(|| base_common.clone());
    let guest = load_player_options_section(
        ini.section_has_any(GUEST_PLAYER_OPTIONS_SECTION),
        |key| ini.get(GUEST_PLAYER_OPTIONS_SECTION, key),
        &common,
    )
    .unwrap_or_else(|| common.clone());
    let new_profile = load_player_options_section(
        ini.section_has_any(NEW_PROFILE_PLAYER_OPTIONS_SECTION),
        |key| ini.get(NEW_PROFILE_PLAYER_OPTIONS_SECTION, key),
        &common,
    )
    .unwrap_or_else(|| common.clone());
    MachinePlayerDefaults {
        common,
        guest,
        new_profile,
    }
}

impl Profile {
    fn original_apply_player_options(&mut self, options: &PlayerOptionsData) {
        self.background_filter = options.background_filter;
        self.hold_judgment_graphic = options.hold_judgment_graphic.clone();
        self.held_miss_graphic = options.held_miss_graphic.clone();
        self.judgment_graphic = options.judgment_graphic.clone();
        self.combo_font = options.combo_font;
        self.combo_colors = options.combo_colors;
        self.combo_mode = options.combo_mode;
        self.carry_combo_between_songs = options.carry_combo_between_songs;
        self.noteskin = options.noteskin.clone();
        self.arrow_noteskin.clone_from(&options.arrow_noteskin);
        self.hold_active_noteskin
            .clone_from(&options.hold_active_noteskin);
        self.hold_inactive_noteskin
            .clone_from(&options.hold_inactive_noteskin);
        self.roll_active_noteskin
            .clone_from(&options.roll_active_noteskin);
        self.roll_inactive_noteskin
            .clone_from(&options.roll_inactive_noteskin);
        self.hold_explosion_noteskin
            .clone_from(&options.hold_explosion_noteskin);
        self.lift_noteskin.clone_from(&options.lift_noteskin);
        self.mine_size_percent = options.mine_size_percent;
        self.mine_noteskin.clone_from(&options.mine_noteskin);
        self.receptor_noteskin
            .clone_from(&options.receptor_noteskin);
        self.tap_explosion_noteskin
            .clone_from(&options.tap_explosion_noteskin);
        self.tap_explosion_active_mask = options.tap_explosion_active_mask;
        self.scroll_speed = options.scroll_speed;
        self.no_cmod_alternative = options.no_cmod_alternative;
        self.scroll_option = options.scroll_option;
        self.reverse_scroll = options.reverse_scroll;
        self.turn_option = options.turn_option;
        self.insert_active_mask = options.insert_active_mask;
        self.remove_active_mask = options.remove_active_mask;
        self.holds_active_mask = options.holds_active_mask;
        self.accel_effects_active_mask = options.accel_effects_active_mask;
        self.visual_effects_active_mask = options.visual_effects_active_mask;
        self.appearance_effects_active_mask = options.appearance_effects_active_mask;
        self.attack_mode = options.attack_mode;
        self.hide_light_type = options.hide_light_type;
        self.rescore_early_hits = options.rescore_early_hits;
        self.hide_early_dw_judgments = options.hide_early_dw_judgments;
        self.hide_early_dw_flash = options.hide_early_dw_flash;
        self.hide_early_dw_column_flash = options.hide_early_dw_column_flash;
        self.timing_windows = options.timing_windows;
        self.show_fa_plus_window = options.show_fa_plus_window;
        self.show_ex_score = options.show_ex_score;
        self.show_hard_ex_score = options.show_hard_ex_score;
        self.show_fa_plus_pane = options.show_fa_plus_pane;
        self.fa_plus_10ms_blue_window = options.fa_plus_10ms_blue_window;
        self.split_15_10ms = options.split_15_10ms;
        self.track_early_judgments = options.track_early_judgments;
        self.scale_scatterplot = options.scale_scatterplot;
        self.dim_post_fail_scatter = options.dim_post_fail_scatter;
        self.scatterplot_max_window = options.scatterplot_max_window;
        self.score_position = options.score_position;
        self.score_display_mode = options.score_display_mode;
        self.custom_fantastic_window = options.custom_fantastic_window;
        self.custom_fantastic_window_ms = options.custom_fantastic_window_ms;
        self.pad_light_brightness = options.pad_light_brightness;
        self.judgment_tilt = options.judgment_tilt;
        self.column_cues = options.column_cues;
        self.measure_cues = options.measure_cues;
        self.crossover_cues = options.crossover_cues;
        self.crossover_cue_duration_ms = options.crossover_cue_duration_ms;
        self.crossover_cue_quantization = options.crossover_cue_quantization;
        self.crossover_cue_brackets = options.crossover_cue_brackets;
        self.column_countdown = options.column_countdown;
        self.judgment_back = options.judgment_back;
        self.error_ms_display = options.error_ms_display;
        self.display_scorebox = options.display_scorebox;
        self.live_timing_stats = options.live_timing_stats;
        self.live_timing_stats_mask = options.live_timing_stats_mask;
        self.rainbow_max = options.rainbow_max;
        self.responsive_colors = options.responsive_colors;
        self.show_life_percent = options.show_life_percent;
        self.tilt_multiplier = options.tilt_multiplier;
        self.tilt_min_threshold_ms = options.tilt_min_threshold_ms;
        self.tilt_max_threshold_ms = options.tilt_max_threshold_ms;
        self.error_bar_active_mask = options.error_bar_active_mask;
        self.error_bar = options.error_bar;
        self.error_bar_text = options.error_bar_text;
        self.text_error_bar_scalable = options.text_error_bar_scalable;
        self.text_error_bar_threshold_ms = options.text_error_bar_threshold_ms;
        self.error_bar_up = options.error_bar_up;
        self.error_bar_multi_tick = options.error_bar_multi_tick;
        self.error_bar_trim = options.error_bar_trim;
        self.center_tick = options.center_tick;
        self.short_average_error_bar_enabled = options.short_average_error_bar_enabled;
        self.average_error_bar_intensity = options.average_error_bar_intensity;
        self.average_error_bar_interval_ms = options.average_error_bar_interval_ms;
        self.long_error_bar_enabled = options.long_error_bar_enabled;
        self.long_error_bar_intensity = options.long_error_bar_intensity;
        self.long_error_bar_threshold_ms = options.long_error_bar_threshold_ms;
        self.long_error_bar_min_samples = options.long_error_bar_min_samples;
        self.step_statistics = options.step_statistics;
        self.step_stats_extra = options.step_stats_extra.clone();
        self.target_score = options.target_score;
        self.target_score_percent = options.target_score_percent;
        self.target_score_miss_policy = options.target_score_miss_policy;
        self.lifemeter_type = options.lifemeter_type;
        self.measure_counter = options.measure_counter;
        self.measure_counter_lookahead = options.measure_counter_lookahead;
        self.measure_counter_left = options.measure_counter_left;
        self.measure_counter_up = options.measure_counter_up;
        self.measure_counter_vert = options.measure_counter_vert;
        self.broken_run = options.broken_run;
        self.run_timer = options.run_timer;
        self.measure_lines = options.measure_lines;
        self.hide_targets = options.hide_targets;
        self.hide_song_bg = options.hide_song_bg;
        self.hide_combo = options.hide_combo;
        self.hide_lifebar = options.hide_lifebar;
        self.hide_score = options.hide_score;
        self.hide_danger = options.hide_danger;
        self.hide_combo_explosions = options.hide_combo_explosions;
        self.hide_username = options.hide_username;
        self.column_flash_on_miss = options.column_flash_on_miss;
        self.column_flash_mask = options.column_flash_mask;
        self.column_flash_brightness = options.column_flash_brightness;
        self.column_flash_size = options.column_flash_size;
        self.subtractive_scoring = options.subtractive_scoring;
        self.pacemaker = options.pacemaker;
        self.nps_graph_at_top = options.nps_graph_at_top;
        self.transparent_density_graph_bg = options.transparent_density_graph_bg;
        self.smx_fsr_display = options.smx_fsr_display;
        self.smx_pad_input_display = options.smx_pad_input_display;
        self.smx_bg_pack.clone_from(&options.smx_bg_pack);
        self.smx_judge_pack.clone_from(&options.smx_judge_pack);
        self.mini_indicator = options.mini_indicator;
        self.mini_indicator_score_type = options.mini_indicator_score_type;
        self.mini_indicator_subtractive_display = options.mini_indicator_subtractive_display;
        self.mini_indicator_size = options.mini_indicator_size;
        self.mini_indicator_color = options.mini_indicator_color;
        self.mini_indicator_position = options.mini_indicator_position;
        self.mini_percent = options.mini_percent;
        self.spacing_percent = options.spacing_percent;
        self.perspective = options.perspective;
        self.note_field_offset_x = options.note_field_offset_x;
        self.note_field_offset_y = options.note_field_offset_y;
        self.judgment_offset_x = options.judgment_offset_x;
        self.judgment_offset_y = options.judgment_offset_y;
        self.combo_offset_x = options.combo_offset_x;
        self.combo_offset_y = options.combo_offset_y;
        self.error_bar_offset_x = options.error_bar_offset_x;
        self.error_bar_offset_y = options.error_bar_offset_y;
        self.visual_delay_ms = options.visual_delay_ms;
        self.global_offset_shift_ms = options.global_offset_shift_ms;
    }
    pub fn original_set_current_player_options(&mut self, options: PlayerOptionsData) -> bool {
        if self.current_player_options() == options {
            return false;
        }
        self.original_apply_player_options(&options);
        true
    }
    pub fn original_apply_player_options_for_style(&mut self, style: PlayStyle) {
        let options = self.player_options(style).clone();
        self.original_apply_player_options(&options);
    }
}
