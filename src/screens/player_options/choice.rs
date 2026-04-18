use super::*;
use crate::game::profile::{AttackMode, BackgroundFilter, ComboColors, ComboFont, ComboMode, DataVisualizations, ErrorBarTrim, HideLightType, LifeMeterType, MeasureCounter, MeasureLines, MiniIndicator, MiniIndicatorScoreType, Perspective, TargetScoreSetting, TimingWindowsOption, TurnOption};

pub(super) fn change_choice_for_player(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) {
    if state.row_map.is_empty() {
        return;
    }
    let player_idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[player_idx].min(state.row_map.len().saturating_sub(1));
    let id = state.row_map.row(state.row_map.id_at(row_index)).id;
    if id == RowId::Exit {
        return;
    }
    let is_shared = row_is_shared(id);

    // Shared row: Music Rate
    if id == RowId::MusicRate {
        let row = state.row_map.row_mut(state.row_map.id_at(row_index));
        let increment = 0.01f32;
        let min_rate = 0.05f32;
        let max_rate = 3.00f32;
        state.music_rate += delta as f32 * increment;
        state.music_rate = (state.music_rate / increment).round() * increment;
        state.music_rate = state.music_rate.clamp(min_rate, max_rate);
        row.choices[0] = fmt_music_rate(state.music_rate);

        audio::play_sfx("assets/sounds/change_value.ogg");
        crate::game::profile::set_session_music_rate(state.music_rate);
        audio::set_music_rate(state.music_rate);
        return;
    }

    // Per-player row: Speed Mod numeric
    if id == RowId::SpeedMod {
        let speed_mod = {
            let speed_mod = &mut state.speed_mod[player_idx];
            let (upper, increment) = match speed_mod.mod_type.as_str() {
                "X" => (20.0, 0.05),
                "C" | "M" => (2000.0, 5.0),
                _ => (1.0, 0.1),
            };
            speed_mod.value += delta as f32 * increment;
            speed_mod.value = (speed_mod.value / increment).round() * increment;
            speed_mod.value = speed_mod.value.clamp(increment, upper);
            speed_mod.clone()
        };
        sync_profile_scroll_speed(&mut state.player_profiles[player_idx], &speed_mod);
        audio::play_sfx("assets/sounds/change_value.ogg");
        return;
    }

    let play_style = crate::game::profile::get_session_play_style();
    let persisted_idx = session_persisted_player_idx();
    let should_persist =
        play_style == crate::game::profile::PlayStyle::Versus || player_idx == persisted_idx;
    let persist_side = if player_idx == P1 {
        crate::game::profile::PlayerSide::P1
    } else {
        crate::game::profile::PlayerSide::P2
    };

    let row = state.row_map.row_mut(state.row_map.id_at(row_index));
    let num_choices = row.choices.len();
    if num_choices == 0 {
        return;
    }
    let mut visibility_changed = false;

    let current_idx = row.selected_choice_index[player_idx] as isize;
    let new_index = ((current_idx + delta + num_choices as isize) % num_choices as isize) as usize;

    if is_shared {
        row.selected_choice_index = [new_index; PLAYER_SLOTS];
    } else {
        row.selected_choice_index[player_idx] = new_index;
    }

    if id == RowId::TypeOfSpeedMod {
        let new_type = match row.selected_choice_index[player_idx] {
            0 => "X",
            1 => "C",
            2 => "M",
            _ => "C",
        };

        let speed_mod = &mut state.speed_mod[player_idx];
        let old_type = speed_mod.mod_type.clone();
        let old_value = speed_mod.value;
        let reference_bpm = reference_bpm_for_song(
            &state.song,
            resolve_p1_chart(&state.song, &state.chart_steps_index),
        );
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let target_bpm: f32 = match old_type.as_str() {
            "C" | "M" => old_value,
            "X" => (reference_bpm * rate * old_value).round(),
            _ => 600.0,
        };
        let new_value = match new_type {
            "X" => {
                let denom = reference_bpm * rate;
                let raw = if denom.is_finite() && denom > 0.0 {
                    target_bpm / denom
                } else {
                    1.0
                };
                let stepped = round_to_step(raw, 0.05);
                stepped.clamp(0.05, 20.0)
            }
            "C" | "M" => {
                let stepped = round_to_step(target_bpm, 5.0);
                stepped.clamp(5.0, 2000.0)
            }
            _ => 600.0,
        };
        speed_mod.mod_type = new_type.to_string();
        speed_mod.value = new_value;
        let speed_mod = speed_mod.clone();
        sync_profile_scroll_speed(&mut state.player_profiles[player_idx], &speed_mod);
    } else if id == RowId::Turn {
        let setting = TURN_OPTION_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(TurnOption::None);
        state.player_profiles[player_idx].turn_option = setting;
        if should_persist {
            crate::game::profile::update_turn_option_for_side(persist_side, setting);
        }
    } else if id == RowId::Accel || id == RowId::Effect || id == RowId::Appearance {
        // Multi-select rows toggled with Start; Left/Right only moves cursor.
    } else if id == RowId::Attacks {
        let setting = ATTACK_MODE_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(AttackMode::On);
        state.player_profiles[player_idx].attack_mode = setting;
        if should_persist {
            crate::game::profile::update_attack_mode_for_side(persist_side, setting);
        }
    } else if id == RowId::HideLightType {
        let setting = HIDE_LIGHT_TYPE_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(HideLightType::NoHideLights);
        state.player_profiles[player_idx].hide_light_type = setting;
        if should_persist {
            crate::game::profile::update_hide_light_type_for_side(persist_side, setting);
        }
    } else if id == RowId::RescoreEarlyHits {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].rescore_early_hits = enabled;
        if should_persist {
            crate::game::profile::update_rescore_early_hits_for_side(persist_side, enabled);
        }
    } else if id == RowId::TimingWindows {
        let setting = TIMING_WINDOWS_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(TimingWindowsOption::None);
        state.player_profiles[player_idx].timing_windows = setting;
        if should_persist {
            crate::game::profile::update_timing_windows_for_side(persist_side, setting);
        }
    } else if id == RowId::CustomBlueFantasticWindow {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].custom_fantastic_window = enabled;
        if should_persist {
            crate::game::profile::update_custom_fantastic_window_for_side(persist_side, enabled);
        }
        visibility_changed = true;
    } else if id == RowId::CustomBlueFantasticWindowMs {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.trim_end_matches("ms").parse::<u8>()
        {
            let ms = crate::game::profile::clamp_custom_fantastic_window_ms(raw);
            state.player_profiles[player_idx].custom_fantastic_window_ms = ms;
            if should_persist {
                crate::game::profile::update_custom_fantastic_window_ms_for_side(persist_side, ms);
            }
        }
    } else if id == RowId::MiniIndicator {
        let choice_idx =
            row.selected_choice_index[player_idx].min(row.choices.len().saturating_sub(1));
        let mini_indicator = MINI_INDICATOR_VARIANTS
            .get(choice_idx)
            .copied()
            .unwrap_or(MiniIndicator::None);
        let subtractive_scoring = mini_indicator == MiniIndicator::SubtractiveScoring;
        let pacemaker = mini_indicator == MiniIndicator::Pacemaker;
        state.player_profiles[player_idx].mini_indicator = mini_indicator;
        state.player_profiles[player_idx].subtractive_scoring = subtractive_scoring;
        state.player_profiles[player_idx].pacemaker = pacemaker;

        if should_persist {
            let profile_ref = &state.player_profiles[player_idx];
            crate::game::profile::update_mini_indicator_for_side(persist_side, mini_indicator);
            crate::game::profile::update_gameplay_extras_for_side(
                persist_side,
                profile_ref.column_flash_on_miss,
                subtractive_scoring,
                pacemaker,
                profile_ref.nps_graph_at_top,
            );
        }
        visibility_changed = true;
    } else if id == RowId::IndicatorScoreType {
        let score_type = MINI_INDICATOR_SCORE_TYPE_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(MiniIndicatorScoreType::Itg);
        state.player_profiles[player_idx].mini_indicator_score_type = score_type;
        if should_persist {
            crate::game::profile::update_mini_indicator_score_type_for_side(
                persist_side,
                score_type,
            );
        }
    } else if id == RowId::DensityGraphBackground {
        let transparent = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].transparent_density_graph_bg = transparent;
        if should_persist {
            crate::game::profile::update_transparent_density_graph_bg_for_side(
                persist_side,
                transparent,
            );
        }
    } else if id == RowId::BackgroundFilter {
        let setting = BACKGROUND_FILTER_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(BackgroundFilter::Darkest);
        state.player_profiles[player_idx].background_filter = setting;
        if should_persist {
            crate::game::profile::update_background_filter_for_side(persist_side, setting);
        }
    } else if id == RowId::Mini {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx]) {
            let trimmed = choice.trim_end_matches('%');
            if let Ok(val) = trimmed.parse::<i32>() {
                state.player_profiles[player_idx].mini_percent = val;
                if should_persist {
                    crate::game::profile::update_mini_percent_for_side(persist_side, val);
                }
            }
        }
    } else if id == RowId::Perspective {
        let setting = PERSPECTIVE_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(Perspective::Overhead);
        state.player_profiles[player_idx].perspective = setting;
        if should_persist {
            crate::game::profile::update_perspective_for_side(persist_side, setting);
        }
    } else if id == RowId::NoteFieldOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].note_field_offset_x = raw;
            if should_persist {
                crate::game::profile::update_notefield_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::NoteFieldOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].note_field_offset_y = raw;
            if should_persist {
                crate::game::profile::update_notefield_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::JudgmentOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].judgment_offset_x = raw;
            if should_persist {
                crate::game::profile::update_judgment_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::JudgmentOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].judgment_offset_y = raw;
            if should_persist {
                crate::game::profile::update_judgment_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ComboOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].combo_offset_x = raw;
            if should_persist {
                crate::game::profile::update_combo_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ComboOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].combo_offset_y = raw;
            if should_persist {
                crate::game::profile::update_combo_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ErrorBarOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].error_bar_offset_x = raw;
            if should_persist {
                crate::game::profile::update_error_bar_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ErrorBarOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].error_bar_offset_y = raw;
            if should_persist {
                crate::game::profile::update_error_bar_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::VisualDelay {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.trim_end_matches("ms").parse::<i32>()
        {
            state.player_profiles[player_idx].visual_delay_ms = raw;
            if should_persist {
                crate::game::profile::update_visual_delay_ms_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::GlobalOffsetShift {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.trim_end_matches("ms").parse::<i32>()
        {
            state.player_profiles[player_idx].global_offset_shift_ms = raw;
            if should_persist {
                crate::game::profile::update_global_offset_shift_ms_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::JudgmentTilt {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].judgment_tilt = enabled;
        if should_persist {
            crate::game::profile::update_judgment_tilt_for_side(persist_side, enabled);
        }
        visibility_changed = true;
    } else if id == RowId::JudgmentTiltIntensity {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(mult) = choice.parse::<f32>()
        {
            let mult = round_to_step(mult, TILT_INTENSITY_STEP)
                .clamp(TILT_INTENSITY_MIN, TILT_INTENSITY_MAX);
            state.player_profiles[player_idx].tilt_multiplier = mult;
            if should_persist {
                crate::game::profile::update_tilt_multiplier_for_side(persist_side, mult);
            }
        }
    } else if id == RowId::JudgmentBehindArrows {
        let enabled = row.selected_choice_index[player_idx] != 0;
        state.player_profiles[player_idx].judgment_back = enabled;
        if should_persist {
            crate::game::profile::update_judgment_back_for_side(persist_side, enabled);
        }
    } else if id == RowId::LifeMeterType {
        let setting = LIFE_METER_TYPE_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(LifeMeterType::Standard);
        state.player_profiles[player_idx].lifemeter_type = setting;
        if should_persist {
            crate::game::profile::update_lifemeter_type_for_side(persist_side, setting);
        }
    } else if id == RowId::LifeBarOptions {
        // Multi-select row toggled with Start; Left/Right only moves cursor.
    } else if id == RowId::DataVisualizations {
        let setting = DATA_VISUALIZATIONS_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(DataVisualizations::None);
        state.player_profiles[player_idx].data_visualizations = setting;
        if should_persist {
            crate::game::profile::update_data_visualizations_for_side(persist_side, setting);
        }
        visibility_changed = true;
    } else if id == RowId::TargetScore {
        let setting = TARGET_SCORE_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(TargetScoreSetting::S);
        state.player_profiles[player_idx].target_score = setting;
        if should_persist {
            crate::game::profile::update_target_score_for_side(persist_side, setting);
        }
    } else if id == RowId::OffsetIndicator {
        let enabled = row.selected_choice_index[player_idx] != 0;
        state.player_profiles[player_idx].error_ms_display = enabled;
        if should_persist {
            crate::game::profile::update_error_ms_display_for_side(persist_side, enabled);
        }
    } else if id == RowId::ErrorBar {
        // Multi-select row toggled with Start; Left/Right only moves cursor.
    } else if id == RowId::ErrorBarTrim {
        let setting = ERROR_BAR_TRIM_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(ErrorBarTrim::Off);
        state.player_profiles[player_idx].error_bar_trim = setting;
        if should_persist {
            crate::game::profile::update_error_bar_trim_for_side(persist_side, setting);
        }
    } else if id == RowId::MeasureCounter {
        visibility_changed = true;
        let setting = MEASURE_COUNTER_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(MeasureCounter::None);
        state.player_profiles[player_idx].measure_counter = setting;
        if should_persist {
            crate::game::profile::update_measure_counter_for_side(persist_side, setting);
        }
    } else if id == RowId::MeasureCounterLookahead {
        let lookahead = (row.selected_choice_index[player_idx] as u8).min(4);
        state.player_profiles[player_idx].measure_counter_lookahead = lookahead;
        if should_persist {
            crate::game::profile::update_measure_counter_lookahead_for_side(
                persist_side,
                lookahead,
            );
        }
    } else if id == RowId::MeasureLines {
        let setting = MEASURE_LINES_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(MeasureLines::Off);
        state.player_profiles[player_idx].measure_lines = setting;
        if should_persist {
            crate::game::profile::update_measure_lines_for_side(persist_side, setting);
        }
    } else if id == RowId::JudgmentFont {
        let setting = assets::judgment_texture_choices()
            .get(row.selected_choice_index[player_idx])
            .map(|choice| crate::game::profile::JudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].judgment_graphic = setting;
        if should_persist {
            crate::game::profile::update_judgment_graphic_for_side(
                persist_side,
                state.player_profiles[player_idx].judgment_graphic.clone(),
            );
        }
        visibility_changed = true;
    } else if id == RowId::ComboFont {
        let setting = COMBO_FONT_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(ComboFont::Wendy);
        state.player_profiles[player_idx].combo_font = setting;
        if should_persist {
            crate::game::profile::update_combo_font_for_side(persist_side, setting);
        }
        visibility_changed = true;
    } else if id == RowId::ComboColors {
        let setting = COMBO_COLORS_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(ComboColors::Glow);
        state.player_profiles[player_idx].combo_colors = setting;
        if should_persist {
            crate::game::profile::update_combo_colors_for_side(persist_side, setting);
        }
    } else if id == RowId::ComboColorMode {
        let setting = COMBO_MODE_VARIANTS
            .get(row.selected_choice_index[player_idx])
            .copied()
            .unwrap_or(ComboMode::FullCombo);
        state.player_profiles[player_idx].combo_mode = setting;
        if should_persist {
            crate::game::profile::update_combo_mode_for_side(persist_side, setting);
        }
    } else if id == RowId::CarryCombo {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].carry_combo_between_songs = enabled;
        if should_persist {
            crate::game::profile::update_carry_combo_between_songs_for_side(persist_side, enabled);
        }
    } else if id == RowId::HoldJudgment {
        let setting = assets::hold_judgment_texture_choices()
            .get(row.selected_choice_index[player_idx])
            .map(|choice| crate::game::profile::HoldJudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].hold_judgment_graphic = setting;
        if should_persist {
            crate::game::profile::update_hold_judgment_graphic_for_side(
                persist_side,
                state.player_profiles[player_idx]
                    .hold_judgment_graphic
                    .clone(),
            );
        }
    } else if id == RowId::NoteSkin {
        let setting_name = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .cloned()
            .unwrap_or_else(|| crate::game::profile::NoteSkin::DEFAULT_NAME.to_string());
        let setting = crate::game::profile::NoteSkin::new(&setting_name);
        state.player_profiles[player_idx].noteskin = setting.clone();
        if should_persist {
            crate::game::profile::update_noteskin_for_side(persist_side, setting.clone());
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::MineSkin {
        let match_noteskin = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
        let selected = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(String::as_str)
            .unwrap_or(match_noteskin.as_ref());
        let setting = if selected == match_noteskin.as_ref() {
            None
        } else {
            Some(crate::game::profile::NoteSkin::new(selected))
        };
        state.player_profiles[player_idx]
            .mine_noteskin
            .clone_from(&setting);
        if should_persist {
            crate::game::profile::update_mine_noteskin_for_side(persist_side, setting);
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::ReceptorSkin {
        let match_noteskin = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
        let selected = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(String::as_str)
            .unwrap_or(match_noteskin.as_ref());
        let setting = if selected == match_noteskin.as_ref() {
            None
        } else {
            Some(crate::game::profile::NoteSkin::new(selected))
        };
        state.player_profiles[player_idx]
            .receptor_noteskin
            .clone_from(&setting);
        if should_persist {
            crate::game::profile::update_receptor_noteskin_for_side(persist_side, setting);
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::TapExplosionSkin {
        let match_noteskin = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
        let no_tap_explosion = tr("PlayerOptions", NO_TAP_EXPLOSION_LABEL);
        let selected = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(String::as_str)
            .unwrap_or(match_noteskin.as_ref());
        let setting = if selected == match_noteskin.as_ref() {
            None
        } else if selected == no_tap_explosion.as_ref() {
            Some(crate::game::profile::NoteSkin::none_choice())
        } else {
            Some(crate::game::profile::NoteSkin::new(selected))
        };
        state.player_profiles[player_idx]
            .tap_explosion_noteskin
            .clone_from(&setting);
        if should_persist {
            crate::game::profile::update_tap_explosion_noteskin_for_side(persist_side, setting);
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::Stepchart
        && let Some(diff_indices) = &row.choice_difficulty_indices
        && let Some(&difficulty_idx) = diff_indices.get(row.selected_choice_index[player_idx])
    {
        state.chart_steps_index[player_idx] = difficulty_idx;
        if difficulty_idx < crate::engine::present::color::FILE_DIFFICULTY_NAMES.len() {
            state.chart_difficulty_index[player_idx] = difficulty_idx;
        }
    }

    if visibility_changed {
        sync_selected_rows_with_visibility(state, session_active_players());
    }
    sync_inline_intent_from_row(state, asset_manager, player_idx, row_index);
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub fn apply_choice_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) {
    if state.row_map.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.selected_row[idx].min(state.row_map.len().saturating_sub(1));
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.row_map.get(id))
        && row_supports_inline_nav(row)
    {
        if state.current_pane == OptionsPane::Main || row_selects_on_focus_move(row.id) {
            change_choice_for_player(state, asset_manager, idx, delta);
            return;
        }
        if move_inline_focus(state, asset_manager, idx, delta) {
            audio::play_sfx("assets/sounds/change_value.ogg");
        }
        return;
    }
    change_choice_for_player(state, asset_manager, player_idx, delta);
}

pub(super) fn toggle_scroll_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Scroll {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.scroll_active_mask[idx] & bit) != 0 {
        state.scroll_active_mask[idx] &= !bit;
    } else {
        state.scroll_active_mask[idx] |= bit;
    }

    // Rebuild the ScrollOption bitmask from the active choices.
    use crate::game::profile::ScrollOption;
    let mut setting = ScrollOption::Normal;
    if state.scroll_active_mask[idx] != 0 {
        if (state.scroll_active_mask[idx] & (1u8 << 0)) != 0 {
            setting = setting.union(ScrollOption::Reverse);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 1)) != 0 {
            setting = setting.union(ScrollOption::Split);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 2)) != 0 {
            setting = setting.union(ScrollOption::Alternate);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 3)) != 0 {
            setting = setting.union(ScrollOption::Cross);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 4)) != 0 {
            setting = setting.union(ScrollOption::Centered);
        }
    }
    state.player_profiles[idx].scroll_option = setting;
    state.player_profiles[idx].reverse_scroll = setting.contains(ScrollOption::Reverse);
    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_scroll_option_for_side(side, setting);
    }
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_hide_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Hide {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.hide_active_mask[idx] & bit) != 0 {
        state.hide_active_mask[idx] &= !bit;
    } else {
        state.hide_active_mask[idx] |= bit;
    }

    let hide_targets = (state.hide_active_mask[idx] & (1u8 << 0)) != 0;
    let hide_song_bg = (state.hide_active_mask[idx] & (1u8 << 1)) != 0;
    let hide_combo = (state.hide_active_mask[idx] & (1u8 << 2)) != 0;
    let hide_lifebar = (state.hide_active_mask[idx] & (1u8 << 3)) != 0;
    let hide_score = (state.hide_active_mask[idx] & (1u8 << 4)) != 0;
    let hide_danger = (state.hide_active_mask[idx] & (1u8 << 5)) != 0;
    let hide_combo_explosions = (state.hide_active_mask[idx] & (1u8 << 6)) != 0;

    state.player_profiles[idx].hide_targets = hide_targets;
    state.player_profiles[idx].hide_song_bg = hide_song_bg;
    state.player_profiles[idx].hide_combo = hide_combo;
    state.player_profiles[idx].hide_lifebar = hide_lifebar;
    state.player_profiles[idx].hide_score = hide_score;
    state.player_profiles[idx].hide_danger = hide_danger;
    state.player_profiles[idx].hide_combo_explosions = hide_combo_explosions;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_hide_options_for_side(
            side,
            hide_targets,
            hide_song_bg,
            hide_combo,
            hide_lifebar,
            hide_score,
            hide_danger,
            hide_combo_explosions,
        );
    }

    sync_selected_rows_with_visibility(state, session_active_players());
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_insert_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Insert {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 7 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.insert_active_mask[idx] & bit) != 0 {
        state.insert_active_mask[idx] &= !bit;
    } else {
        state.insert_active_mask[idx] |= bit;
    }
    state.insert_active_mask[idx] =
        crate::game::profile::normalize_insert_mask(state.insert_active_mask[idx]);
    let mask = state.insert_active_mask[idx];
    state.player_profiles[idx].insert_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_insert_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_remove_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Remove {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.remove_active_mask[idx] & bit) != 0 {
        state.remove_active_mask[idx] &= !bit;
    } else {
        state.remove_active_mask[idx] |= bit;
    }
    state.remove_active_mask[idx] =
        crate::game::profile::normalize_remove_mask(state.remove_active_mask[idx]);
    let mask = state.remove_active_mask[idx];
    state.player_profiles[idx].remove_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_remove_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_holds_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Holds {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .row_map
            .row(state.row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.holds_active_mask[idx] & bit) != 0 {
        state.holds_active_mask[idx] &= !bit;
    } else {
        state.holds_active_mask[idx] |= bit;
    }
    state.holds_active_mask[idx] =
        crate::game::profile::normalize_holds_mask(state.holds_active_mask[idx]);
    let mask = state.holds_active_mask[idx];
    state.player_profiles[idx].holds_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_holds_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_accel_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Accel {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .row_map
            .row(state.row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.accel_effects_active_mask[idx] & bit) != 0 {
        state.accel_effects_active_mask[idx] &= !bit;
    } else {
        state.accel_effects_active_mask[idx] |= bit;
    }
    state.accel_effects_active_mask[idx] =
        crate::game::profile::normalize_accel_effects_mask(state.accel_effects_active_mask[idx]);
    let mask = state.accel_effects_active_mask[idx];
    state.player_profiles[idx].accel_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_accel_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_visual_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Effect {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 10 {
        1u16 << (choice_index as u16)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.visual_effects_active_mask[idx] & bit) != 0 {
        state.visual_effects_active_mask[idx] &= !bit;
    } else {
        state.visual_effects_active_mask[idx] |= bit;
    }
    state.visual_effects_active_mask[idx] =
        crate::game::profile::normalize_visual_effects_mask(state.visual_effects_active_mask[idx]);
    let mask = state.visual_effects_active_mask[idx];
    state.player_profiles[idx].visual_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_visual_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_appearance_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::Appearance {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .row_map
            .row(state.row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.appearance_effects_active_mask[idx] & bit) != 0 {
        state.appearance_effects_active_mask[idx] &= !bit;
    } else {
        state.appearance_effects_active_mask[idx] |= bit;
    }
    state.appearance_effects_active_mask[idx] =
        crate::game::profile::normalize_appearance_effects_mask(
            state.appearance_effects_active_mask[idx],
        );
    let mask = state.appearance_effects_active_mask[idx];
    state.player_profiles[idx].appearance_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_appearance_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_life_bar_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::LifeBarOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 3 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.life_bar_options_active_mask[idx] & bit) != 0 {
        state.life_bar_options_active_mask[idx] &= !bit;
    } else {
        state.life_bar_options_active_mask[idx] |= bit;
    }

    let rainbow_max = (state.life_bar_options_active_mask[idx] & (1u8 << 0)) != 0;
    let responsive_colors = (state.life_bar_options_active_mask[idx] & (1u8 << 1)) != 0;
    let show_life_percent = (state.life_bar_options_active_mask[idx] & (1u8 << 2)) != 0;
    state.player_profiles[idx].rainbow_max = rainbow_max;
    state.player_profiles[idx].responsive_colors = responsive_colors;
    state.player_profiles[idx].show_life_percent = show_life_percent;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_rainbow_max_for_side(side, rainbow_max);
        crate::game::profile::update_responsive_colors_for_side(side, responsive_colors);
        crate::game::profile::update_show_life_percent_for_side(side, show_life_percent);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_fa_plus_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::FAPlusOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index
        < state
            .row_map
            .row(state.row_map.id_at(row_index))
            .choices
            .len()
            .min(u8::BITS as usize)
    {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.fa_plus_active_mask[idx] & bit) != 0 {
        state.fa_plus_active_mask[idx] &= !bit;
    } else {
        state.fa_plus_active_mask[idx] |= bit;
    }

    let window_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 0)) != 0;
    let ex_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 1)) != 0;
    let hard_ex_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 2)) != 0;
    let pane_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 3)) != 0;
    let ten_ms_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 4)) != 0;
    let split_15_10ms_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 5)) != 0;
    state.player_profiles[idx].show_fa_plus_window = window_enabled;
    state.player_profiles[idx].show_ex_score = ex_enabled;
    state.player_profiles[idx].show_hard_ex_score = hard_ex_enabled;
    state.player_profiles[idx].show_fa_plus_pane = pane_enabled;
    state.player_profiles[idx].fa_plus_10ms_blue_window = ten_ms_enabled;
    state.player_profiles[idx].split_15_10ms = split_15_10ms_enabled;
    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_show_fa_plus_window_for_side(side, window_enabled);
        crate::game::profile::update_show_ex_score_for_side(side, ex_enabled);
        crate::game::profile::update_show_hard_ex_score_for_side(side, hard_ex_enabled);
        crate::game::profile::update_show_fa_plus_pane_for_side(side, pane_enabled);
        crate::game::profile::update_fa_plus_10ms_blue_window_for_side(side, ten_ms_enabled);
        crate::game::profile::update_split_15_10ms_for_side(side, split_15_10ms_enabled);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_results_extras_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::ResultsExtras {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 1 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.results_extras_active_mask[idx] & bit) != 0 {
        state.results_extras_active_mask[idx] &= !bit;
    } else {
        state.results_extras_active_mask[idx] |= bit;
    }

    let track_early_judgments = (state.results_extras_active_mask[idx] & (1u8 << 0)) != 0;
    state.player_profiles[idx].track_early_judgments = track_early_judgments;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_track_early_judgments_for_side(side, track_early_judgments);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_error_bar_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::ErrorBar {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 5 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.error_bar_active_mask[idx] & bit) != 0 {
        state.error_bar_active_mask[idx] &= !bit;
    } else {
        state.error_bar_active_mask[idx] |= bit;
    }
    state.error_bar_active_mask[idx] =
        crate::game::profile::normalize_error_bar_mask(state.error_bar_active_mask[idx]);
    let mask = state.error_bar_active_mask[idx];
    state.player_profiles[idx].error_bar_active_mask = mask;
    state.player_profiles[idx].error_bar = crate::game::profile::error_bar_style_from_mask(mask);
    state.player_profiles[idx].error_bar_text =
        crate::game::profile::error_bar_text_from_mask(mask);

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_error_bar_mask_for_side(side, mask);
    }

    sync_selected_rows_with_visibility(state, session_active_players());
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_error_bar_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::ErrorBarOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 2 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.error_bar_options_active_mask[idx] & bit) != 0 {
        state.error_bar_options_active_mask[idx] &= !bit;
    } else {
        state.error_bar_options_active_mask[idx] |= bit;
    }

    let up = (state.error_bar_options_active_mask[idx] & (1u8 << 0)) != 0;
    let multi_tick = (state.error_bar_options_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].error_bar_up = up;
    state.player_profiles[idx].error_bar_multi_tick = multi_tick;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_error_bar_options_for_side(side, up, multi_tick);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_measure_counter_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::MeasureCounterOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 5 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.measure_counter_options_active_mask[idx] & bit) != 0 {
        state.measure_counter_options_active_mask[idx] &= !bit;
    } else {
        state.measure_counter_options_active_mask[idx] |= bit;
    }

    let left = (state.measure_counter_options_active_mask[idx] & (1u8 << 0)) != 0;
    let up = (state.measure_counter_options_active_mask[idx] & (1u8 << 1)) != 0;
    let vert = (state.measure_counter_options_active_mask[idx] & (1u8 << 2)) != 0;
    let broken_run = (state.measure_counter_options_active_mask[idx] & (1u8 << 3)) != 0;
    let run_timer = (state.measure_counter_options_active_mask[idx] & (1u8 << 4)) != 0;

    state.player_profiles[idx].measure_counter_left = left;
    state.player_profiles[idx].measure_counter_up = up;
    state.player_profiles[idx].measure_counter_vert = vert;
    state.player_profiles[idx].broken_run = broken_run;
    state.player_profiles[idx].run_timer = run_timer;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_measure_counter_options_for_side(
            side, left, up, vert, broken_run, run_timer,
        );
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_early_dw_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::EarlyDecentWayOffOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = if choice_index < 2 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.early_dw_active_mask[idx] & bit) != 0 {
        state.early_dw_active_mask[idx] &= !bit;
    } else {
        state.early_dw_active_mask[idx] |= bit;
    }

    let hide_judgments = (state.early_dw_active_mask[idx] & (1u8 << 0)) != 0;
    let hide_flash = (state.early_dw_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].hide_early_dw_judgments = hide_judgments;
    state.player_profiles[idx].hide_early_dw_flash = hide_flash;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_early_dw_options_for_side(side, hide_judgments, hide_flash);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_gameplay_extras_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::GameplayExtras {
            return;
        }
    } else {
        return;
    }

    let row = state.row_map.row(state.row_map.id_at(row_index));
    let choice_index = row.selected_choice_index[idx];
    let ge_flash = tr("PlayerOptions", "GameplayExtrasFlashColumnForMiss");
    let ge_density = tr("PlayerOptions", "GameplayExtrasDensityGraphAtTop");
    let ge_column_cues = tr("PlayerOptions", "GameplayExtrasColumnCues");
    let ge_scorebox = tr("PlayerOptions", "GameplayExtrasDisplayScorebox");
    let bit = row
        .choices
        .get(choice_index)
        .map(|choice| {
            let choice_str = choice.as_str();
            if choice_str == ge_flash.as_ref() {
                1u8 << 0
            } else if choice_str == ge_density.as_ref() {
                1u8 << 1
            } else if choice_str == ge_column_cues.as_ref() {
                1u8 << 2
            } else if choice_str == ge_scorebox.as_ref() {
                1u8 << 3
            } else {
                0
            }
        })
        .unwrap_or(0);
    if bit == 0 {
        return;
    }

    if (state.gameplay_extras_active_mask[idx] & bit) != 0 {
        state.gameplay_extras_active_mask[idx] &= !bit;
    } else {
        state.gameplay_extras_active_mask[idx] |= bit;
    }

    let column_flash_on_miss = (state.gameplay_extras_active_mask[idx] & (1u8 << 0)) != 0;
    let nps_graph_at_top = (state.gameplay_extras_active_mask[idx] & (1u8 << 1)) != 0;
    let column_cues = (state.gameplay_extras_active_mask[idx] & (1u8 << 2)) != 0;
    let display_scorebox = (state.gameplay_extras_active_mask[idx] & (1u8 << 3)) != 0;
    let subtractive_scoring = state.player_profiles[idx].subtractive_scoring;
    let pacemaker = state.player_profiles[idx].pacemaker;

    state.player_profiles[idx].column_flash_on_miss = column_flash_on_miss;
    state.player_profiles[idx].nps_graph_at_top = nps_graph_at_top;
    state.player_profiles[idx].column_cues = column_cues;
    state.player_profiles[idx].display_scorebox = display_scorebox;
    state.gameplay_extras_more_active_mask[idx] =
        (column_cues as u8) | ((display_scorebox as u8) << 1);

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_gameplay_extras_for_side(
            side,
            column_flash_on_miss,
            subtractive_scoring,
            pacemaker,
            nps_graph_at_top,
        );
        crate::game::profile::update_column_cues_for_side(side, column_cues);
        crate::game::profile::update_display_scorebox_for_side(side, display_scorebox);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn toggle_gameplay_extras_more_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state
        .row_map
        .display_order()
        .get(row_index)
        .and_then(|&id| state.row_map.get(id))
    {
        if row.id != RowId::GameplayExtrasMore {
            return;
        }
    } else {
        return;
    }

    let choice_index = state
        .row_map
        .row(state.row_map.id_at(row_index))
        .selected_choice_index[idx];
    let bit = match choice_index {
        0 => 1u8 << 0, // Column Cues
        1 => 1u8 << 1, // Display Scorebox
        _ => return,
    };

    if (state.gameplay_extras_more_active_mask[idx] & bit) != 0 {
        state.gameplay_extras_more_active_mask[idx] &= !bit;
    } else {
        state.gameplay_extras_more_active_mask[idx] |= bit;
    }

    let column_cues = (state.gameplay_extras_more_active_mask[idx] & (1u8 << 0)) != 0;
    let display_scorebox = (state.gameplay_extras_more_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].column_cues = column_cues;
    state.player_profiles[idx].display_scorebox = display_scorebox;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_column_cues_for_side(side, column_cues);
        crate::game::profile::update_display_scorebox_for_side(side, display_scorebox);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub(super) fn apply_pane(state: &mut State, pane: OptionsPane) {
    let speed_mod = &state.speed_mod[session_persisted_player_idx()];
    let mut row_map = build_rows(
        &state.song,
        speed_mod,
        state.chart_steps_index,
        state.chart_difficulty_index,
        state.music_rate,
        pane,
        &state.noteskin_names,
        state.return_screen,
        state.fixed_stepchart.as_ref(),
    );
    let (
        scroll_active_mask_p1,
        hide_active_mask_p1,
        insert_active_mask_p1,
        remove_active_mask_p1,
        holds_active_mask_p1,
        accel_effects_active_mask_p1,
        visual_effects_active_mask_p1,
        appearance_effects_active_mask_p1,
        fa_plus_active_mask_p1,
        early_dw_active_mask_p1,
        gameplay_extras_active_mask_p1,
        gameplay_extras_more_active_mask_p1,
        results_extras_active_mask_p1,
        life_bar_options_active_mask_p1,
        error_bar_active_mask_p1,
        error_bar_options_active_mask_p1,
        measure_counter_options_active_mask_p1,
    ) = apply_profile_defaults(&mut row_map, &state.player_profiles[P1], P1);
    let (
        scroll_active_mask_p2,
        hide_active_mask_p2,
        insert_active_mask_p2,
        remove_active_mask_p2,
        holds_active_mask_p2,
        accel_effects_active_mask_p2,
        visual_effects_active_mask_p2,
        appearance_effects_active_mask_p2,
        fa_plus_active_mask_p2,
        early_dw_active_mask_p2,
        gameplay_extras_active_mask_p2,
        gameplay_extras_more_active_mask_p2,
        results_extras_active_mask_p2,
        life_bar_options_active_mask_p2,
        error_bar_active_mask_p2,
        error_bar_options_active_mask_p2,
        measure_counter_options_active_mask_p2,
    ) = apply_profile_defaults(&mut row_map, &state.player_profiles[P2], P2);
    state.row_map = row_map;
    state.scroll_active_mask = [scroll_active_mask_p1, scroll_active_mask_p2];
    state.hide_active_mask = [hide_active_mask_p1, hide_active_mask_p2];
    state.insert_active_mask = [insert_active_mask_p1, insert_active_mask_p2];
    state.remove_active_mask = [remove_active_mask_p1, remove_active_mask_p2];
    state.holds_active_mask = [holds_active_mask_p1, holds_active_mask_p2];
    state.accel_effects_active_mask = [accel_effects_active_mask_p1, accel_effects_active_mask_p2];
    state.visual_effects_active_mask =
        [visual_effects_active_mask_p1, visual_effects_active_mask_p2];
    state.appearance_effects_active_mask = [
        appearance_effects_active_mask_p1,
        appearance_effects_active_mask_p2,
    ];
    state.fa_plus_active_mask = [fa_plus_active_mask_p1, fa_plus_active_mask_p2];
    state.early_dw_active_mask = [early_dw_active_mask_p1, early_dw_active_mask_p2];
    state.gameplay_extras_active_mask = [
        gameplay_extras_active_mask_p1,
        gameplay_extras_active_mask_p2,
    ];
    state.gameplay_extras_more_active_mask = [
        gameplay_extras_more_active_mask_p1,
        gameplay_extras_more_active_mask_p2,
    ];
    state.results_extras_active_mask =
        [results_extras_active_mask_p1, results_extras_active_mask_p2];
    state.life_bar_options_active_mask = [
        life_bar_options_active_mask_p1,
        life_bar_options_active_mask_p2,
    ];
    state.error_bar_active_mask = [error_bar_active_mask_p1, error_bar_active_mask_p2];
    state.error_bar_options_active_mask = [
        error_bar_options_active_mask_p1,
        error_bar_options_active_mask_p2,
    ];
    state.measure_counter_options_active_mask = [
        measure_counter_options_active_mask_p1,
        measure_counter_options_active_mask_p2,
    ];
    state.current_pane = pane;
    state.selected_row = [0; PLAYER_SLOTS];
    state.prev_selected_row = [0; PLAYER_SLOTS];
    state.inline_choice_x = [f32::NAN; PLAYER_SLOTS];
    state.arcade_row_focus = [false; PLAYER_SLOTS];
    state.start_held_since = [None; PLAYER_SLOTS];
    state.start_last_triggered_at = [None; PLAYER_SLOTS];
    state.cursor_initialized = [false; PLAYER_SLOTS];
    state.cursor_from_x = [0.0; PLAYER_SLOTS];
    state.cursor_from_y = [0.0; PLAYER_SLOTS];
    state.cursor_from_w = [0.0; PLAYER_SLOTS];
    state.cursor_from_h = [0.0; PLAYER_SLOTS];
    state.cursor_to_x = [0.0; PLAYER_SLOTS];
    state.cursor_to_y = [0.0; PLAYER_SLOTS];
    state.cursor_to_w = [0.0; PLAYER_SLOTS];
    state.cursor_to_h = [0.0; PLAYER_SLOTS];
    state.cursor_t = [1.0; PLAYER_SLOTS];
    state.help_anim_time = [0.0; PLAYER_SLOTS];
    let active = session_active_players();
    state.row_tweens = init_row_tweens(
        &state.row_map,
        state.selected_row,
        active,
        state.hide_active_mask,
        state.error_bar_active_mask,
        state.allow_per_player_global_offsets,
    );
    state.arcade_row_focus = std::array::from_fn(|player_idx| {
        row_allows_arcade_next_row(state, state.selected_row[player_idx])
    });
}

pub(super) fn switch_to_pane(state: &mut State, pane: OptionsPane) {
    if state.current_pane == pane {
        return;
    }
    audio::play_sfx("assets/sounds/start.ogg");

    state.nav_key_held_direction = [None; PLAYER_SLOTS];
    state.nav_key_held_since = [None; PLAYER_SLOTS];
    state.nav_key_last_scrolled_at = [None; PLAYER_SLOTS];
    state.start_held_since = [None; PLAYER_SLOTS];
    state.start_last_triggered_at = [None; PLAYER_SLOTS];

    state.pane_transition = match state.pane_transition {
        PaneTransition::FadingOut { t, .. } => PaneTransition::FadingOut { target: pane, t },
        _ => PaneTransition::FadingOut {
            target: pane,
            t: 0.0,
        },
    };
}

