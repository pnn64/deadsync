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
    wrap: NavWrap,
) -> Outcome {
    match super::choice::cycle_choice_index(state, player_idx, id, delta, wrap) {
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
    init_opted_in_cycle_rows(row_map, profile, player_idx);
    init_opted_in_numeric_rows(row_map, profile, player_idx);
    apply_derived_masks(profile, masks);

    let match_ns_label = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
    let no_tap_label = tr("PlayerOptions", NO_TAP_EXPLOSION_LABEL);
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
    // Initialize Judgment Tilt rows from profile (Simply Love semantics).
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
    if let Some(row) = row_map.get_mut(RowId::JudgmentTiltMinThreshold) {
        let threshold =
            crate::game::profile::clamp_tilt_threshold_ms(profile.tilt_min_threshold_ms);
        let needle = fmt_tilt_threshold_ms(threshold);
        row.selected_choice_index[player_idx] = row
            .choices
            .iter()
            .position(|c| c == &needle)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::JudgmentTiltMaxThreshold) {
        let min_threshold =
            crate::game::profile::clamp_tilt_threshold_ms(profile.tilt_min_threshold_ms);
        let threshold =
            crate::game::profile::clamp_tilt_threshold_ms(profile.tilt_max_threshold_ms)
                .max(min_threshold);
        let needle = fmt_tilt_threshold_ms(threshold);
        row.selected_choice_index[player_idx] = row
            .choices
            .iter()
            .position(|c| c == &needle)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::MeasureCounterLookahead) {
        row.selected_choice_index[player_idx] = (profile.measure_counter_lookahead.min(4) as usize)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = row_map.get_mut(RowId::MiniIndicator) {
        row.selected_choice_index[player_idx] = MINI_INDICATOR_VARIANTS
            .iter()
            .position(|&v| v == profile.mini_indicator)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
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
        super::row::init_bitmask_row_from_binding(row, &binding, profile, masks, player_idx);
    }
}

fn init_opted_in_cycle_rows(
    row_map: &mut RowMap,
    profile: &crate::game::profile::Profile,
    player_idx: usize,
) {
    let ids: Vec<RowId> = row_map.display_order().to_vec();
    for id in ids {
        let Some(row) = row_map.get_mut(id) else {
            continue;
        };
        match row.behavior {
            RowBehavior::Cycle(super::row::CycleBinding::Index(binding)) => {
                super::row::init_cycle_row_from_binding(row, &binding, profile, player_idx);
            }
            RowBehavior::Cycle(super::row::CycleBinding::Bool(binding)) => {
                super::row::init_cycle_row_from_binding(row, &binding, profile, player_idx);
            }
            _ => {}
        }
    }
}

fn init_opted_in_numeric_rows(
    row_map: &mut RowMap,
    profile: &crate::game::profile::Profile,
    player_idx: usize,
) {
    let ids: Vec<RowId> = row_map.display_order().to_vec();
    for id in ids {
        let Some(row) = row_map.get(id) else {
            continue;
        };
        let RowBehavior::Numeric(binding) = row.behavior else {
            continue;
        };
        if binding.init.is_none() {
            continue;
        }
        let row = row_map.get_mut(id).expect("row was just observed");
        super::row::init_numeric_row_from_binding(row, &binding, profile, player_idx);
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

fn apply_derived_masks(profile: &crate::game::profile::Profile, masks: &mut PlayerOptionMasks) {
    for rule in DERIVED_MASKS {
        (rule.apply)(profile, masks);
    }
}
