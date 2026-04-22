use super::*;

mod advanced;
mod main;
mod uncommon;
use advanced::*;
use main::*;
use uncommon::*;

/// Cycle binding for the "What Comes Next" row. Mirroring across players is
/// handled by the dispatcher via `Row::mirror_across_players`, not here.
pub(super) const WHAT_COMES_NEXT: CustomBinding = CustomBinding {
    apply: apply_what_comes_next_cycle,
};

fn apply_what_comes_next_cycle(
    state: &mut State,
    player_idx: usize,
    id: RowId,
    delta: isize,
) -> Outcome {
    match super::choice::cycle_choice_index(state, player_idx, id, delta) {
        Some(_) => Outcome::persisted(),
        None => Outcome::NONE,
    }
}

#[inline(always)]
pub(super) fn choose_different_screen_label(return_screen: Screen) -> String {
    match return_screen {
        Screen::SelectCourse => tr("PlayerOptions", "ChooseDifferentCourse").to_string(),
        _ => tr("PlayerOptions", "ChooseDifferentSong").to_string(),
    }
}

pub(super) fn what_comes_next_choices(pane: OptionsPane, return_screen: Screen) -> Vec<String> {
    let choose_different = choose_different_screen_label(return_screen);
    match pane {
        OptionsPane::Main => vec![
            tr("PlayerOptions", "WhatComesNextGameplay").to_string(),
            choose_different,
            tr("PlayerOptions", "WhatComesNextAdvancedModifiers").to_string(),
            tr("PlayerOptions", "WhatComesNextUncommonModifiers").to_string(),
        ],
        OptionsPane::Advanced => vec![
            tr("PlayerOptions", "WhatComesNextGameplay").to_string(),
            choose_different,
            tr("PlayerOptions", "WhatComesNextMainModifiers").to_string(),
            tr("PlayerOptions", "WhatComesNextUncommonModifiers").to_string(),
        ],
        OptionsPane::Uncommon => vec![
            tr("PlayerOptions", "WhatComesNextGameplay").to_string(),
            choose_different,
            tr("PlayerOptions", "WhatComesNextMainModifiers").to_string(),
            tr("PlayerOptions", "WhatComesNextAdvancedModifiers").to_string(),
        ],
    }
}

pub(super) fn build_rows(
    song: &SongData,
    speed_mod: &SpeedMod,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    session_music_rate: f32,
    pane: OptionsPane,
    noteskin_names: &[String],
    return_screen: Screen,
    fixed_stepchart: Option<&FixedStepchart>,
) -> RowMap {
    match pane {
        OptionsPane::Main => build_main_rows(
            song,
            speed_mod,
            chart_steps_index,
            preferred_difficulty_index,
            session_music_rate,
            noteskin_names,
            return_screen,
            fixed_stepchart,
        ),
        OptionsPane::Advanced => build_advanced_rows(return_screen),
        OptionsPane::Uncommon => build_uncommon_rows(return_screen),
    }
}

fn find_noteskin_choice_index(
    profile_value: Option<&crate::game::profile::NoteSkin>,
    choices: &[String],
    match_label: &str,
    none_label: Option<&str>,
) -> usize {
    let position_eq = |label: &str| choices.iter().position(|c| c.as_str() == label);
    match profile_value {
        None => position_eq(match_label).unwrap_or(0),
        Some(skin) => {
            if let Some(none_label) = none_label {
                if skin.is_none_choice() {
                    return position_eq(none_label).unwrap_or(0);
                }
            }
            choices
                .iter()
                .position(|c| c.eq_ignore_ascii_case(skin.as_str()))
                .or_else(|| position_eq(match_label))
                .unwrap_or(0)
        }
    }
}

pub(super) fn apply_profile_defaults(
    row_map: &mut RowMap,
    profile: &crate::game::profile::Profile,
    player_idx: usize,
) -> PlayerOptionMasks {
    let mut scroll_active_mask = ScrollMask::empty();
    let mut hide_active_mask = HideMask::empty();
    let mut insert_active_mask = InsertMask::empty();
    let mut remove_active_mask = RemoveMask::empty();
    let mut holds_active_mask = HoldsMask::empty();
    let mut accel_effects_active_mask = AccelEffectsMask::empty();
    let mut visual_effects_active_mask = VisualEffectsMask::empty();
    let mut appearance_effects_active_mask = AppearanceEffectsMask::empty();
    let mut fa_plus_active_mask = FaPlusMask::empty();
    let mut early_dw_active_mask = EarlyDwMask::empty();
    let mut gameplay_extras_active_mask = GameplayExtrasMask::empty();
    let mut gameplay_extras_more_active_mask = GameplayExtrasMoreMask::empty();
    let mut results_extras_active_mask = ResultsExtrasMask::empty();
    let mut life_bar_options_active_mask = LifeBarOptionsMask::empty();
    let mut error_bar_active_mask = profile.error_bar_active_mask;
    if error_bar_active_mask.is_empty() {
        error_bar_active_mask = crate::game::profile::error_bar_mask_from_style(
            profile.error_bar,
            profile.error_bar_text,
        );
    }
    let mut error_bar_options_active_mask = ErrorBarOptionsMask::empty();
    let mut measure_counter_options_active_mask = MeasureCounterOptionsMask::empty();
    let match_ns_label = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
    let no_tap_label = tr("PlayerOptions", NO_TAP_EXPLOSION_LABEL);
    // Initialize Background Filter row from profile setting (0..=100 %).
    if let Some(row) = row_map.get_mut(RowId::BackgroundFilter) {
        row.selected_choice_index[player_idx] =
            (profile.background_filter.percent() as usize).min(row.choices.len().saturating_sub(1));
    }
    // Initialize Judgment Font row from profile setting
    if let Some(row) = row_map.get_mut(RowId::JudgmentFont) {
        row.selected_choice_index[player_idx] = assets::judgment_texture_choices()
            .iter()
            .position(|choice| {
                choice
                    .key
                    .eq_ignore_ascii_case(profile.judgment_graphic.as_str())
            })
            .unwrap_or(0);
    }
    // Initialize NoteSkin row from profile setting
    if let Some(row) = row_map.get_mut(RowId::NoteSkin) {
        row.selected_choice_index[player_idx] = row
            .choices
            .iter()
            .position(|c| c.eq_ignore_ascii_case(profile.noteskin.as_str()))
            .or_else(|| {
                row.choices.iter().position(|c| {
                    c.eq_ignore_ascii_case(crate::game::profile::NoteSkin::DEFAULT_NAME)
                })
            })
            .unwrap_or(0);
    }
    if let Some(row) = row_map.get_mut(RowId::MineSkin) {
        row.selected_choice_index[player_idx] = find_noteskin_choice_index(
            profile.mine_noteskin.as_ref(),
            &row.choices,
            match_ns_label.as_ref(),
            None,
        );
    }
    if let Some(row) = row_map.get_mut(RowId::ReceptorSkin) {
        row.selected_choice_index[player_idx] = find_noteskin_choice_index(
            profile.receptor_noteskin.as_ref(),
            &row.choices,
            match_ns_label.as_ref(),
            None,
        );
    }
    if let Some(row) = row_map.get_mut(RowId::TapExplosionSkin) {
        row.selected_choice_index[player_idx] = find_noteskin_choice_index(
            profile.tap_explosion_noteskin.as_ref(),
            &row.choices,
            match_ns_label.as_ref(),
            Some(no_tap_label.as_ref()),
        );
    }
    // Initialize Combo Font row from profile setting
    if let Some(row) = row_map.get_mut(RowId::ComboFont) {
        row.selected_choice_index[player_idx] = COMBO_FONT_VARIANTS
            .iter()
            .position(|&v| v == profile.combo_font)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::ComboColors) {
        row.selected_choice_index[player_idx] = COMBO_COLORS_VARIANTS
            .iter()
            .position(|&v| v == profile.combo_colors)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::ComboColorMode) {
        row.selected_choice_index[player_idx] = COMBO_MODE_VARIANTS
            .iter()
            .position(|&v| v == profile.combo_mode)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::CarryCombo) {
        row.selected_choice_index[player_idx] = if profile.carry_combo_between_songs {
            1
        } else {
            0
        };
    }
    // Initialize Hold Judgment row from profile setting (Love, mute, ITG2, None)
    if let Some(row) = row_map.get_mut(RowId::HoldJudgment) {
        row.selected_choice_index[player_idx] = assets::hold_judgment_texture_choices()
            .iter()
            .position(|choice| {
                choice
                    .key
                    .eq_ignore_ascii_case(profile.hold_judgment_graphic.as_str())
            })
            .unwrap_or(0);
    }
    // Initialize Mini row from profile (range -100..150, stored as percent).
    if let Some(row) = row_map.get_mut(RowId::Mini) {
        let val = profile.mini_percent.clamp(-100, 150);
        let needle = format!("{val}%");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Perspective row from profile setting (Overhead, Hallway, Distant, Incoming, Space).
    if let Some(row) = row_map.get_mut(RowId::Perspective) {
        row.selected_choice_index[player_idx] = PERSPECTIVE_VARIANTS
            .iter()
            .position(|&v| v == profile.perspective)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    // Initialize NoteField Offset X from profile (0..50, non-negative; P1 uses negative sign at render time)
    if let Some(row) = row_map.get_mut(RowId::NoteFieldOffsetX) {
        let val = profile.note_field_offset_x.clamp(0, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize NoteField Offset Y from profile (-50..50)
    if let Some(row) = row_map.get_mut(RowId::NoteFieldOffsetY) {
        let val = profile.note_field_offset_y.clamp(-50, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Judgment Offset X from profile (HUD offset range)
    if let Some(row) = row_map.get_mut(RowId::JudgmentOffsetX) {
        let val = profile
            .judgment_offset_x
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Judgment Offset Y from profile (HUD offset range)
    if let Some(row) = row_map.get_mut(RowId::JudgmentOffsetY) {
        let val = profile
            .judgment_offset_y
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Combo Offset X from profile (HUD offset range)
    if let Some(row) = row_map.get_mut(RowId::ComboOffsetX) {
        let val = profile.combo_offset_x.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Combo Offset Y from profile (HUD offset range)
    if let Some(row) = row_map.get_mut(RowId::ComboOffsetY) {
        let val = profile.combo_offset_y.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Error Bar Offset X from profile (HUD offset range)
    if let Some(row) = row_map.get_mut(RowId::ErrorBarOffsetX) {
        let val = profile
            .error_bar_offset_x
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Error Bar Offset Y from profile (HUD offset range)
    if let Some(row) = row_map.get_mut(RowId::ErrorBarOffsetY) {
        let val = profile
            .error_bar_offset_y
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Visual Delay from profile (-100..100ms)
    if let Some(row) = row_map.get_mut(RowId::VisualDelay) {
        let val = profile.visual_delay_ms.clamp(-100, 100);
        let needle = format!("{val}ms");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::GlobalOffsetShift) {
        let val = profile.global_offset_shift_ms.clamp(-100, 100);
        let needle = format!("{val}ms");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Judgment Tilt rows from profile (Simply Love semantics).
    if let Some(row) = row_map.get_mut(RowId::JudgmentTilt) {
        row.selected_choice_index[player_idx] = if profile.judgment_tilt { 1 } else { 0 };
    }
    if let Some(row) = row_map.get_mut(RowId::JudgmentTiltIntensity) {
        let stepped = round_to_step(
            profile
                .tilt_multiplier
                .clamp(TILT_INTENSITY_MIN, TILT_INTENSITY_MAX),
            TILT_INTENSITY_STEP,
        )
        .clamp(TILT_INTENSITY_MIN, TILT_INTENSITY_MAX);
        let needle = fmt_tilt_intensity(stepped);
        row.selected_choice_index[player_idx] = row
            .choices
            .iter()
            .position(|c| c == &needle)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::JudgmentBehindArrows) {
        row.selected_choice_index[player_idx] = if profile.judgment_back { 1 } else { 0 };
    }
    // Initialize Error Bar rows from profile (Simply Love semantics).
    if let Some(row) = row_map.get_mut(RowId::OffsetIndicator) {
        row.selected_choice_index[player_idx] = if profile.error_ms_display { 1 } else { 0 };
    }
    if let Some(row) = row_map.get_mut(RowId::ErrorBar) {
        if !error_bar_active_mask.is_empty() {
            let bits = error_bar_active_mask.bits();
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (bits & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::DataVisualizations) {
        row.selected_choice_index[player_idx] = DATA_VISUALIZATIONS_VARIANTS
            .iter()
            .position(|&v| v == profile.data_visualizations)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::TargetScore) {
        row.selected_choice_index[player_idx] = TARGET_SCORE_VARIANTS
            .iter()
            .position(|&v| v == profile.target_score)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::LifeMeterType) {
        row.selected_choice_index[player_idx] = LIFE_METER_TYPE_VARIANTS
            .iter()
            .position(|&v| v == profile.lifemeter_type)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if profile.rainbow_max {
        life_bar_options_active_mask.insert(LifeBarOptionsMask::RAINBOW_MAX);
    }
    if profile.responsive_colors {
        life_bar_options_active_mask.insert(LifeBarOptionsMask::RESPONSIVE_COLORS);
    }
    if profile.show_life_percent {
        life_bar_options_active_mask.insert(LifeBarOptionsMask::SHOW_LIFE_PERCENT);
    }
    if let Some(row) = row_map.get_mut(RowId::LifeBarOptions) {
        if !life_bar_options_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (life_bar_options_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::ErrorBarTrim) {
        row.selected_choice_index[player_idx] = ERROR_BAR_TRIM_VARIANTS
            .iter()
            .position(|&v| v == profile.error_bar_trim)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if profile.error_bar_up {
        error_bar_options_active_mask.insert(ErrorBarOptionsMask::MOVE_UP);
    }
    if profile.error_bar_multi_tick {
        error_bar_options_active_mask.insert(ErrorBarOptionsMask::MULTI_TICK);
    }
    if let Some(row) = row_map.get_mut(RowId::ErrorBarOptions) {
        if !error_bar_options_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (error_bar_options_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    // Initialize Measure Counter rows (zmod semantics).
    if let Some(row) = row_map.get_mut(RowId::MeasureCounter) {
        row.selected_choice_index[player_idx] = MEASURE_COUNTER_VARIANTS
            .iter()
            .position(|&v| v == profile.measure_counter)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::MeasureCounterLookahead) {
        row.selected_choice_index[player_idx] = (profile.measure_counter_lookahead.min(4) as usize)
            .min(row.choices.len().saturating_sub(1));
    }
    if profile.measure_counter_left {
        measure_counter_options_active_mask.insert(MeasureCounterOptionsMask::MOVE_LEFT);
    }
    if profile.measure_counter_up {
        measure_counter_options_active_mask.insert(MeasureCounterOptionsMask::MOVE_UP);
    }
    if profile.measure_counter_vert {
        measure_counter_options_active_mask.insert(MeasureCounterOptionsMask::VERTICAL_LOOKAHEAD);
    }
    if profile.broken_run {
        measure_counter_options_active_mask.insert(MeasureCounterOptionsMask::BROKEN_RUN_TOTAL);
    }
    if profile.run_timer {
        measure_counter_options_active_mask.insert(MeasureCounterOptionsMask::RUN_TIMER);
    }
    if let Some(row) = row_map.get_mut(RowId::MeasureCounterOptions) {
        if !measure_counter_options_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (measure_counter_options_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::MeasureLines) {
        row.selected_choice_index[player_idx] = MEASURE_LINES_VARIANTS
            .iter()
            .position(|&v| v == profile.measure_lines)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    // Initialize Turn row from profile setting.
    if let Some(row) = row_map.get_mut(RowId::Turn) {
        row.selected_choice_index[player_idx] = TURN_OPTION_VARIANTS
            .iter()
            .position(|&v| v == profile.turn_option)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::RescoreEarlyHits) {
        row.selected_choice_index[player_idx] = if profile.rescore_early_hits { 1 } else { 0 };
    }
    if let Some(row) = row_map.get_mut(RowId::TimingWindows) {
        row.selected_choice_index[player_idx] = TIMING_WINDOWS_VARIANTS
            .iter()
            .position(|&v| v == profile.timing_windows)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if profile.track_early_judgments {
        results_extras_active_mask.insert(ResultsExtrasMask::TRACK_EARLY_JUDGMENTS);
    }
    if let Some(row) = row_map.get_mut(RowId::ResultsExtras) {
        if !results_extras_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (results_extras_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::MiniIndicator) {
        row.selected_choice_index[player_idx] = MINI_INDICATOR_VARIANTS
            .iter()
            .position(|&v| v == profile.mini_indicator)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::IndicatorScoreType) {
        row.selected_choice_index[player_idx] = MINI_INDICATOR_SCORE_TYPE_VARIANTS
            .iter()
            .position(|&v| v == profile.mini_indicator_score_type)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::EarlyDecentWayOffOptions) {
        if profile.hide_early_dw_judgments {
            early_dw_active_mask.insert(EarlyDwMask::HIDE_JUDGMENTS);
        }
        if profile.hide_early_dw_flash {
            early_dw_active_mask.insert(EarlyDwMask::HIDE_FLASH);
        }

        if !early_dw_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (early_dw_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    // Initialize FA+ Options row from profile (independent toggles).
    if let Some(row) = row_map.get_mut(RowId::FAPlusOptions) {
        // Cursor always starts on the first option; toggled state is reflected visually.
        row.selected_choice_index[player_idx] = 0;
    }
    if profile.show_fa_plus_window {
        fa_plus_active_mask.insert(FaPlusMask::WINDOW);
    }
    if profile.show_ex_score {
        fa_plus_active_mask.insert(FaPlusMask::EX_SCORE);
    }
    if profile.show_hard_ex_score {
        fa_plus_active_mask.insert(FaPlusMask::HARD_EX_SCORE);
    }
    if profile.show_fa_plus_pane {
        fa_plus_active_mask.insert(FaPlusMask::PANE);
    }
    if profile.fa_plus_10ms_blue_window {
        fa_plus_active_mask.insert(FaPlusMask::BLUE_WINDOW_10MS);
    }
    if profile.split_15_10ms {
        fa_plus_active_mask.insert(FaPlusMask::SPLIT_15_10MS);
    }
    if let Some(row) = row_map.get_mut(RowId::CustomBlueFantasticWindow) {
        row.selected_choice_index[player_idx] = if profile.custom_fantastic_window {
            1
        } else {
            0
        };
    }
    if let Some(row) = row_map.get_mut(RowId::CustomBlueFantasticWindowMs) {
        let ms = crate::game::profile::clamp_custom_fantastic_window_ms(
            profile.custom_fantastic_window_ms,
        );
        let target = format!("{ms}ms");
        if let Some(idx) = row.choices.iter().position(|c| c == &target) {
            row.selected_choice_index[player_idx] = idx;
        }
    }

    // Initialize Gameplay Extras row from profile (multi-choice toggle group).
    if profile.column_flash_on_miss {
        gameplay_extras_active_mask.insert(GameplayExtrasMask::FLASH_COLUMN_FOR_MISS);
    }
    if profile.nps_graph_at_top {
        gameplay_extras_active_mask.insert(GameplayExtrasMask::DENSITY_GRAPH_AT_TOP);
    }
    if profile.column_cues {
        gameplay_extras_active_mask.insert(GameplayExtrasMask::COLUMN_CUES);
        gameplay_extras_more_active_mask.insert(GameplayExtrasMoreMask::COLUMN_CUES);
    }
    if profile.display_scorebox {
        gameplay_extras_active_mask.insert(GameplayExtrasMask::DISPLAY_SCOREBOX);
        gameplay_extras_more_active_mask.insert(GameplayExtrasMoreMask::DISPLAY_SCOREBOX);
    }
    if let Some(row) = row_map.get_mut(RowId::GameplayExtras) {
        if !gameplay_extras_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (gameplay_extras_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::DensityGraphBackground) {
        row.selected_choice_index[player_idx] = if profile.transparent_density_graph_bg {
            1
        } else {
            0
        };
    }

    // Initialize Gameplay Extras (More) row from profile (multi-choice toggle group).
    if let Some(row) = row_map.get_mut(RowId::GameplayExtrasMore) {
        if !gameplay_extras_more_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (gameplay_extras_more_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }

    // Initialize Hide row from profile (multi-choice toggle group).
    if profile.hide_targets {
        hide_active_mask.insert(HideMask::TARGETS);
    }
    if profile.hide_song_bg {
        hide_active_mask.insert(HideMask::BACKGROUND);
    }
    if profile.hide_combo {
        hide_active_mask.insert(HideMask::COMBO);
    }
    if profile.hide_lifebar {
        hide_active_mask.insert(HideMask::LIFE);
    }
    if profile.hide_score {
        hide_active_mask.insert(HideMask::SCORE);
    }
    if profile.hide_danger {
        hide_active_mask.insert(HideMask::DANGER);
    }
    if profile.hide_combo_explosions {
        hide_active_mask.insert(HideMask::COMBO_EXPLOSIONS);
    }
    if let Some(row) = row_map.get_mut(RowId::Hide) {
        if !hide_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (hide_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }

    // Initialize Scroll row from profile setting (multi-choice toggle group).
    if let Some(row) = row_map.get_mut(RowId::Scroll) {
        use crate::game::profile::ScrollOption;
        // Choice indices are fixed by construction order in build_advanced_rows:
        // 0=Reverse, 1=Split, 2=Alternate, 3=Cross, 4=Centered
        const REVERSE: usize = 0;
        const SPLIT: usize = 1;
        const ALTERNATE: usize = 2;
        const CROSS: usize = 3;
        const CENTERED: usize = 4;
        let flags: &[(ScrollOption, usize)] = &[
            (ScrollOption::Reverse, REVERSE),
            (ScrollOption::Split, SPLIT),
            (ScrollOption::Alternate, ALTERNATE),
            (ScrollOption::Cross, CROSS),
            (ScrollOption::Centered, CENTERED),
        ];
        for &(flag, idx) in flags {
            if profile.scroll_option.contains(flag) && idx < row.choices.len() && idx < 8 {
                scroll_active_mask.insert(ScrollMask::from_bits_truncate(1u8 << (idx as u8)));
            }
        }

        // Cursor starts at the first active choice if any, otherwise at the first option.
        if !scroll_active_mask.is_empty() {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (scroll_active_mask.bits() & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::Insert) {
        insert_active_mask = profile.insert_active_mask;
        let bits = insert_active_mask.bits();
        if bits != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| (bits & (1u8 << (*i as u8))) != 0)
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::Remove) {
        remove_active_mask = profile.remove_active_mask;
        let bits = remove_active_mask.bits();
        if bits != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| (bits & (1u8 << (*i as u8))) != 0)
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::Holds) {
        holds_active_mask = profile.holds_active_mask;
        let bits = holds_active_mask.bits();
        if bits != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| (bits & (1u8 << (*i as u8))) != 0)
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::Accel) {
        accel_effects_active_mask = profile.accel_effects_active_mask;
        let bits = accel_effects_active_mask.bits();
        if bits != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| (bits & (1u8 << (*i as u8))) != 0)
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::Effect) {
        visual_effects_active_mask = profile.visual_effects_active_mask;
        let bits = visual_effects_active_mask.bits();
        if bits != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| (bits & (1u16 << (*i as u16))) != 0)
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::Appearance) {
        appearance_effects_active_mask = profile.appearance_effects_active_mask;
        let bits = appearance_effects_active_mask.bits();
        if bits != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| (bits & (1u8 << (*i as u8))) != 0)
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = row_map.get_mut(RowId::Attacks) {
        row.selected_choice_index[player_idx] = ATTACK_MODE_VARIANTS
            .iter()
            .position(|&v| v == profile.attack_mode)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::HideLightType) {
        row.selected_choice_index[player_idx] = HIDE_LIGHT_TYPE_VARIANTS
            .iter()
            .position(|&v| v == profile.hide_light_type)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    PlayerOptionMasks {
        scroll: scroll_active_mask,
        hide: hide_active_mask,
        insert: insert_active_mask,
        remove: remove_active_mask,
        holds: holds_active_mask,
        accel_effects: accel_effects_active_mask,
        visual_effects: visual_effects_active_mask,
        appearance_effects: appearance_effects_active_mask,
        fa_plus: fa_plus_active_mask,
        early_dw: early_dw_active_mask,
        gameplay_extras: gameplay_extras_active_mask,
        gameplay_extras_more: gameplay_extras_more_active_mask,
        results_extras: results_extras_active_mask,
        life_bar_options: life_bar_options_active_mask,
        error_bar: error_bar_active_mask,
        error_bar_options: error_bar_options_active_mask,
        measure_counter_options: measure_counter_options_active_mask,
    }
}
