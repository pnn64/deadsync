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

/// Initialize per-row cursor positions from `profile` and accumulate any
/// bitmask state into `masks`. Production calls this once per (pane, player)
/// pair, passing the same `&mut PlayerOptionMasks` for both pane calls of a
/// given player so per-pane mask writes accumulate without needing a merge
/// step. Each `BitmaskBinding` writes a disjoint mask field, and the derived
/// pass is a pure function of `profile`, so multiple invocations are safe.
pub(super) fn apply_profile_defaults(
    row_map: &mut RowMap,
    profile: &crate::game::profile::Profile,
    player_idx: usize,
    masks: &mut PlayerOptionMasks,
) {
    init_opted_in_bitmask_rows(row_map, profile, masks, player_idx);
    apply_derived_masks(profile, masks);

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
    if let Some(row) = row_map.get_mut(RowId::JudgmentTiltCutoff) {
        let cutoff = profile
            .tilt_cutoff_ms
            .clamp(TILT_CUTOFF_MIN, TILT_CUTOFF_MAX);

        let needle = format!("{} ms", cutoff);

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
    if let Some(row) = row_map.get_mut(RowId::ErrorBarTrim) {
        row.selected_choice_index[player_idx] = ERROR_BAR_TRIM_VARIANTS
            .iter()
            .position(|&v| v == profile.error_bar_trim)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
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

    if let Some(row) = row_map.get_mut(RowId::DensityGraphBackground) {
        row.selected_choice_index[player_idx] = if profile.transparent_density_graph_bg {
            1
        } else {
            0
        };
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
}

fn init_opted_in_bitmask_rows(
    row_map: &mut RowMap,
    profile: &crate::game::profile::Profile,
    masks: &mut PlayerOptionMasks,
    player_idx: usize,
) {
    let ids: Vec<RowId> = row_map.display_order().to_vec();
    for id in ids {
        let Some(row) = row_map.get(id) else {
            continue;
        };
        let RowBehavior::Bitmask(binding) = row.behavior else {
            continue;
        };
        if binding.init.is_none() {
            continue;
        }
        let row = row_map.get_mut(id).expect("row was just observed");
        super::row::init_bitmask_row_from_binding(
            row, &binding, profile, masks, player_idx,
        );
    }
}

/// Mask fields that are populated as a function of profile state alone, with
/// no user-facing Row of their own. Each rule writes the entire target field
/// based on the current profile, so the order of rules is irrelevant. Run
/// after `init_opted_in_bitmask_rows` so the per-row contracts can no longer
/// stomp derived state.
///
/// To add a derived mask: append a new `DerivedMaskRule` with an `apply`
/// closure that reads the relevant `profile` fields and assigns the target
/// `masks.<field>`. Multiple rules writing the same field are allowed but
/// discouraged; prefer a single closure that builds the full value.
struct DerivedMaskRule {
    apply: fn(&crate::game::profile::Profile, &mut PlayerOptionMasks),
}

const DERIVED_MASKS: &[DerivedMaskRule] = &[DerivedMaskRule {
    // GameplayExtrasMore has no constructed Row; its bits are derived from
    // sibling profile fields that the GameplayExtras row also reads. Keeping
    // both reads in one place prevents the two masks from drifting if a new
    // shared toggle is added later.
    apply: |profile, masks| {
        let mut bits = super::state::GameplayExtrasMoreMask::empty();
        if profile.column_cues {
            bits.insert(super::state::GameplayExtrasMoreMask::COLUMN_CUES);
        }
        if profile.display_scorebox {
            bits.insert(super::state::GameplayExtrasMoreMask::DISPLAY_SCOREBOX);
        }
        masks.gameplay_extras_more = bits;
    },
}];

fn apply_derived_masks(
    profile: &crate::game::profile::Profile,
    masks: &mut PlayerOptionMasks,
) {
    for rule in DERIVED_MASKS {
        (rule.apply)(profile, masks);
    }
}
