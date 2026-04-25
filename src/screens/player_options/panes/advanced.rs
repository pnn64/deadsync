use super::super::constants::MINI_INDICATOR_VARIANTS;
use super::super::row::index_binding;
use super::super::row::{BitmaskInit, CursorInit};
use super::*;
use crate::game::profile as gp;

// =============================== Bindings ===============================

const TURN: ChoiceBinding<usize> = index_binding!(
    TURN_OPTION_VARIANTS,
    gp::TurnOption::None,
    turn_option,
    gp::update_turn_option_for_side,
    false
);
const LIFE_METER_TYPE: ChoiceBinding<usize> = index_binding!(
    LIFE_METER_TYPE_VARIANTS,
    gp::LifeMeterType::Standard,
    lifemeter_type,
    gp::update_lifemeter_type_for_side,
    false
);
const DATA_VISUALIZATIONS: ChoiceBinding<usize> = index_binding!(
    DATA_VISUALIZATIONS_VARIANTS,
    gp::DataVisualizations::None,
    data_visualizations,
    gp::update_data_visualizations_for_side,
    true
);
const TARGET_SCORE: ChoiceBinding<usize> = index_binding!(
    TARGET_SCORE_VARIANTS,
    gp::TargetScoreSetting::S,
    target_score,
    gp::update_target_score_for_side,
    false
);
const INDICATOR_SCORE_TYPE: ChoiceBinding<usize> = index_binding!(
    MINI_INDICATOR_SCORE_TYPE_VARIANTS,
    gp::MiniIndicatorScoreType::Itg,
    mini_indicator_score_type,
    gp::update_mini_indicator_score_type_for_side,
    false
);
const COMBO_COLORS: ChoiceBinding<usize> = index_binding!(
    COMBO_COLORS_VARIANTS,
    gp::ComboColors::Glow,
    combo_colors,
    gp::update_combo_colors_for_side,
    false
);
const COMBO_COLOR_MODE: ChoiceBinding<usize> = index_binding!(
    COMBO_MODE_VARIANTS,
    gp::ComboMode::FullCombo,
    combo_mode,
    gp::update_combo_mode_for_side,
    false
);
const ERROR_BAR_TRIM: ChoiceBinding<usize> = index_binding!(
    ERROR_BAR_TRIM_VARIANTS,
    gp::ErrorBarTrim::Off,
    error_bar_trim,
    gp::update_error_bar_trim_for_side,
    false
);
const MEASURE_COUNTER: ChoiceBinding<usize> = index_binding!(
    MEASURE_COUNTER_VARIANTS,
    gp::MeasureCounter::None,
    measure_counter,
    gp::update_measure_counter_for_side,
    true
);
const MEASURE_LINES: ChoiceBinding<usize> = index_binding!(
    MEASURE_LINES_VARIANTS,
    gp::MeasureLines::Off,
    measure_lines,
    gp::update_measure_lines_for_side,
    false
);
const TIMING_WINDOWS: ChoiceBinding<usize> = index_binding!(
    TIMING_WINDOWS_VARIANTS,
    gp::TimingWindowsOption::None,
    timing_windows,
    gp::update_timing_windows_for_side,
    false
);

const DENSITY_GRAPH_BACKGROUND: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.transparent_density_graph_bg = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_transparent_density_graph_bg_for_side,
};
const CARRY_COMBO: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.carry_combo_between_songs = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_carry_combo_between_songs_for_side,
};
const JUDGMENT_TILT: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.judgment_tilt = v;
        Outcome::persisted_with_visibility()
    },
    persist_for_side: gp::update_judgment_tilt_for_side,
};
const JUDGMENT_BEHIND_ARROWS: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.judgment_back = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_judgment_back_for_side,
};
const OFFSET_INDICATOR: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.error_ms_display = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_error_ms_display_for_side,
};
const RESCORE_EARLY_HITS: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.rescore_early_hits = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_rescore_early_hits_for_side,
};
const CUSTOM_BLUE_FANTASTIC_WINDOW: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.custom_fantastic_window = v;
        Outcome::persisted_with_visibility()
    },
    persist_for_side: gp::update_custom_fantastic_window_for_side,
};

const ERROR_BAR_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.error_bar_offset_x = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_error_bar_offset_x_for_side,
};
const ERROR_BAR_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.error_bar_offset_y = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_error_bar_offset_y_for_side,
};

const SCROLL: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_scroll_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            // The Scroll row's choice indices are fixed by build order
            // (0=Reverse, 1=Split, 2=Alternate, 3=Cross, 4=Centered) and the
            // ScrollOption bit positions match. The order assertion in
            // ``scroll_choice_order_matches_scroll_option_bits`` guards the
            // invariant. We translate per-flag rather than copying ``.0`` so
            // any future divergence is caught here.
            use crate::game::profile::ScrollOption;
            let mut bits = super::super::state::ScrollMask::empty();
            if p.scroll_option.contains(ScrollOption::Reverse) {
                bits.insert(super::super::state::ScrollMask::from_bits_retain(1u8 << 0));
            }
            if p.scroll_option.contains(ScrollOption::Split) {
                bits.insert(super::super::state::ScrollMask::from_bits_retain(1u8 << 1));
            }
            if p.scroll_option.contains(ScrollOption::Alternate) {
                bits.insert(super::super::state::ScrollMask::from_bits_retain(1u8 << 2));
            }
            if p.scroll_option.contains(ScrollOption::Cross) {
                bits.insert(super::super::state::ScrollMask::from_bits_retain(1u8 << 3));
            }
            if p.scroll_option.contains(ScrollOption::Centered) {
                bits.insert(super::super::state::ScrollMask::from_bits_retain(1u8 << 4));
            }
            bits.bits() as u32
        },
        get_active: |m| m.scroll.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "ScrollMask init bits exceed u8 width"
            );
            m.scroll = super::super::state::ScrollMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const HIDE: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_hide_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::HideMask::empty();
            if p.hide_targets {
                bits.insert(super::super::state::HideMask::TARGETS);
            }
            if p.hide_song_bg {
                bits.insert(super::super::state::HideMask::BACKGROUND);
            }
            if p.hide_combo {
                bits.insert(super::super::state::HideMask::COMBO);
            }
            if p.hide_lifebar {
                bits.insert(super::super::state::HideMask::LIFE);
            }
            if p.hide_score {
                bits.insert(super::super::state::HideMask::SCORE);
            }
            if p.hide_danger {
                bits.insert(super::super::state::HideMask::DANGER);
            }
            if p.hide_combo_explosions {
                bits.insert(super::super::state::HideMask::COMBO_EXPLOSIONS);
            }
            bits.bits() as u32
        },
        get_active: |m| m.hide.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "HideMask init bits exceed u8 width"
            );
            m.hide = super::super::state::HideMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const LIFE_BAR_OPTIONS: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_life_bar_options_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::LifeBarOptionsMask::empty();
            if p.rainbow_max {
                bits.insert(super::super::state::LifeBarOptionsMask::RAINBOW_MAX);
            }
            if p.responsive_colors {
                bits.insert(super::super::state::LifeBarOptionsMask::RESPONSIVE_COLORS);
            }
            if p.show_life_percent {
                bits.insert(super::super::state::LifeBarOptionsMask::SHOW_LIFE_PERCENT);
            }
            bits.bits() as u32
        },
        get_active: |m| m.life_bar_options.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "LifeBarOptionsMask init bits exceed u8 width",
            );
            m.life_bar_options = super::super::state::LifeBarOptionsMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const GAMEPLAY_EXTRAS: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_gameplay_extras_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::GameplayExtrasMask::empty();
            if p.column_flash_on_miss {
                bits.insert(super::super::state::GameplayExtrasMask::FLASH_COLUMN_FOR_MISS);
            }
            if p.nps_graph_at_top {
                bits.insert(super::super::state::GameplayExtrasMask::DENSITY_GRAPH_AT_TOP);
            }
            if p.column_cues {
                bits.insert(super::super::state::GameplayExtrasMask::COLUMN_CUES);
            }
            if p.display_scorebox {
                bits.insert(super::super::state::GameplayExtrasMask::DISPLAY_SCOREBOX);
            }
            bits.bits() as u32
        },
        get_active: |m| m.gameplay_extras.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "GameplayExtrasMask init bits exceed u8 width",
            );
            m.gameplay_extras = super::super::state::GameplayExtrasMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const ERROR_BAR: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_error_bar_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            // Profile already stores the desired mask; if it's empty (e.g.
            // legacy profile or unset) fall back to the canonical mapping
            // from the visual style + text-mode pair.
            let mask = if p.error_bar_active_mask.is_empty() {
                crate::game::profile::error_bar_mask_from_style(p.error_bar, p.error_bar_text)
            } else {
                p.error_bar_active_mask
            };
            mask.bits() as u32
        },
        get_active: |m| m.error_bar.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "ErrorBarMask init bits exceed u8 width"
            );
            m.error_bar = crate::game::profile::ErrorBarMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const ERROR_BAR_OPTIONS: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_error_bar_options_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::ErrorBarOptionsMask::empty();
            if p.error_bar_up {
                bits.insert(super::super::state::ErrorBarOptionsMask::MOVE_UP);
            }
            if p.error_bar_multi_tick {
                bits.insert(super::super::state::ErrorBarOptionsMask::MULTI_TICK);
            }
            bits.bits() as u32
        },
        get_active: |m| m.error_bar_options.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "ErrorBarOptionsMask init bits exceed u8 width",
            );
            m.error_bar_options =
                super::super::state::ErrorBarOptionsMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const MEASURE_COUNTER_OPTIONS: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_measure_counter_options_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::MeasureCounterOptionsMask::empty();
            if p.measure_counter_left {
                bits.insert(super::super::state::MeasureCounterOptionsMask::MOVE_LEFT);
            }
            if p.measure_counter_up {
                bits.insert(super::super::state::MeasureCounterOptionsMask::MOVE_UP);
            }
            if p.measure_counter_vert {
                bits.insert(super::super::state::MeasureCounterOptionsMask::VERTICAL_LOOKAHEAD);
            }
            if p.broken_run {
                bits.insert(super::super::state::MeasureCounterOptionsMask::BROKEN_RUN_TOTAL);
            }
            if p.run_timer {
                bits.insert(super::super::state::MeasureCounterOptionsMask::RUN_TIMER);
            }
            bits.bits() as u32
        },
        get_active: |m| m.measure_counter_options.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "MeasureCounterOptionsMask init bits exceed u8 width",
            );
            m.measure_counter_options =
                super::super::state::MeasureCounterOptionsMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const FA_PLUS_OPTIONS: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_fa_plus_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::FaPlusMask::empty();
            if p.show_fa_plus_window {
                bits.insert(super::super::state::FaPlusMask::WINDOW);
            }
            if p.show_ex_score {
                bits.insert(super::super::state::FaPlusMask::EX_SCORE);
            }
            if p.show_hard_ex_score {
                bits.insert(super::super::state::FaPlusMask::HARD_EX_SCORE);
            }
            if p.show_fa_plus_pane {
                bits.insert(super::super::state::FaPlusMask::PANE);
            }
            if p.fa_plus_10ms_blue_window {
                bits.insert(super::super::state::FaPlusMask::BLUE_WINDOW_10MS);
            }
            if p.split_15_10ms {
                bits.insert(super::super::state::FaPlusMask::SPLIT_15_10MS);
            }
            bits.bits() as u32
        },
        get_active: |m| m.fa_plus.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "FaPlusMask init bits exceed u8 width"
            );
            m.fa_plus = super::super::state::FaPlusMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::Fixed(0),
    }),
};
const EARLY_DW_OPTIONS: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_early_dw_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::EarlyDwMask::empty();
            if p.hide_early_dw_judgments {
                bits.insert(super::super::state::EarlyDwMask::HIDE_JUDGMENTS);
            }
            if p.hide_early_dw_flash {
                bits.insert(super::super::state::EarlyDwMask::HIDE_FLASH);
            }
            bits.bits() as u32
        },
        get_active: |m| m.early_dw.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "EarlyDwMask init bits exceed u8 width"
            );
            m.early_dw = super::super::state::EarlyDwMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};
const RESULTS_EXTRAS: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_results_extras_row,
    init: Some(BitmaskInit {
        from_profile: |p| {
            let mut bits = super::super::state::ResultsExtrasMask::empty();
            if p.track_early_judgments {
                bits.insert(super::super::state::ResultsExtrasMask::TRACK_EARLY_JUDGMENTS);
            }
            bits.bits() as u32
        },
        get_active: |m| m.results_extras.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "ResultsExtrasMask init bits exceed u8 width",
            );
            m.results_extras = super::super::state::ResultsExtrasMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    }),
};

const ACTION_ON_MISSED_TARGET: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta| {
        if super::super::choice::cycle_choice_index(state, player_idx, row_id, delta).is_none() {
            return Outcome::NONE;
        }
        Outcome::persisted()
    },
};

const MINI_INDICATOR: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
        else {
            return Outcome::NONE;
        };
        let mini_indicator = MINI_INDICATOR_VARIANTS
            .get(new_index)
            .copied()
            .unwrap_or(gp::MiniIndicator::None);
        let subtractive_scoring = mini_indicator == gp::MiniIndicator::SubtractiveScoring;
        let pacemaker = mini_indicator == gp::MiniIndicator::Pacemaker;
        state.player_profiles[player_idx].mini_indicator = mini_indicator;
        state.player_profiles[player_idx].subtractive_scoring = subtractive_scoring;
        state.player_profiles[player_idx].pacemaker = pacemaker;
        let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
        if should_persist {
            let profile_ref = &state.player_profiles[player_idx];
            gp::update_mini_indicator_for_side(side, mini_indicator);
            gp::update_gameplay_extras_for_side(
                side,
                profile_ref.column_flash_on_miss,
                subtractive_scoring,
                pacemaker,
                profile_ref.nps_graph_at_top,
            );
        }
        Outcome::persisted_with_visibility()
    },
};

const JUDGMENT_TILT_INTENSITY: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
        else {
            return Outcome::NONE;
        };
        let Some(choice) = state
            .pane()
            .row_map
            .get(row_id)
            .and_then(|r| r.choices.get(new_index))
            .cloned()
        else {
            return Outcome::NONE;
        };
        let Ok(mult) = choice.parse::<f32>() else {
            return Outcome::persisted();
        };
        let mult =
            round_to_step(mult, TILT_INTENSITY_STEP).clamp(TILT_INTENSITY_MIN, TILT_INTENSITY_MAX);
        state.player_profiles[player_idx].tilt_multiplier = mult;
        let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
        if should_persist {
            gp::update_tilt_multiplier_for_side(side, mult);
        }
        Outcome::persisted()
    },
};

const MEASURE_COUNTER_LOOKAHEAD: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
        else {
            return Outcome::NONE;
        };
        let lookahead = (new_index as u8).min(4);
        state.player_profiles[player_idx].measure_counter_lookahead = lookahead;
        let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
        if should_persist {
            gp::update_measure_counter_lookahead_for_side(side, lookahead);
        }
        Outcome::persisted()
    },
};

const CUSTOM_BLUE_FANTASTIC_WINDOW_MS: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
        else {
            return Outcome::NONE;
        };
        let Some(choice) = state
            .pane()
            .row_map
            .get(row_id)
            .and_then(|r| r.choices.get(new_index))
            .cloned()
        else {
            return Outcome::NONE;
        };
        let Ok(raw) = choice.trim_end_matches("ms").parse::<u8>() else {
            return Outcome::persisted();
        };
        let ms = gp::clamp_custom_fantastic_window_ms(raw);
        state.player_profiles[player_idx].custom_fantastic_window_ms = ms;
        let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
        if should_persist {
            gp::update_custom_fantastic_window_ms_for_side(side, ms);
        }
        Outcome::persisted()
    },
};

pub(super) fn build_advanced_rows(return_screen: Screen) -> RowMap {
    let mut gameplay_extras_choices = vec![
        tr("PlayerOptions", "GameplayExtrasFlashColumnForMiss").to_string(),
        tr("PlayerOptions", "GameplayExtrasDensityGraphAtTop").to_string(),
        tr("PlayerOptions", "GameplayExtrasColumnCues").to_string(),
    ];
    if crate::game::scores::is_gs_get_scores_service_allowed() {
        gameplay_extras_choices
            .push(tr("PlayerOptions", "GameplayExtrasDisplayScorebox").to_string());
    }

    let mut b = RowBuilder::new();
    b.push(Row {
        id: RowId::Turn,
        behavior: RowBehavior::Cycle(CycleBinding::Index(TURN)),
        name: lookup_key("PlayerOptions", "Turn"),
        choices: vec![
            tr("PlayerOptions", "TurnNone").to_string(),
            tr("PlayerOptions", "TurnMirror").to_string(),
            tr("PlayerOptions", "TurnLeft").to_string(),
            tr("PlayerOptions", "TurnRight").to_string(),
            tr("PlayerOptions", "TurnLRMirror").to_string(),
            tr("PlayerOptions", "TurnUDMirror").to_string(),
            tr("PlayerOptions", "TurnShuffle").to_string(),
            tr("PlayerOptions", "TurnBlender").to_string(),
            tr("PlayerOptions", "TurnRandom").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "TurnHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::Scroll,
        behavior: RowBehavior::Bitmask(SCROLL),
        name: lookup_key("PlayerOptions", "Scroll"),
        choices: vec![
            tr("PlayerOptions", "ScrollReverse").to_string(),
            tr("PlayerOptions", "ScrollSplit").to_string(),
            tr("PlayerOptions", "ScrollAlternate").to_string(),
            tr("PlayerOptions", "ScrollCross").to_string(),
            tr("PlayerOptions", "ScrollCentered").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ScrollHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::Hide,
        behavior: RowBehavior::Bitmask(HIDE),
        name: lookup_key("PlayerOptions", "Hide"),
        choices: vec![
            tr("PlayerOptions", "HideTargets").to_string(),
            tr("PlayerOptions", "HideBackground").to_string(),
            tr("PlayerOptions", "HideCombo").to_string(),
            tr("PlayerOptions", "HideLife").to_string(),
            tr("PlayerOptions", "HideScore").to_string(),
            tr("PlayerOptions", "HideDanger").to_string(),
            tr("PlayerOptions", "HideComboExplosions").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "HideHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::LifeMeterType,
        behavior: RowBehavior::Cycle(CycleBinding::Index(LIFE_METER_TYPE)),
        name: lookup_key("PlayerOptions", "LifeMeterType"),
        choices: vec![
            tr("PlayerOptions", "LifeMeterTypeStandard").to_string(),
            tr("PlayerOptions", "LifeMeterTypeSurround").to_string(),
            tr("PlayerOptions", "LifeMeterTypeVertical").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "LifeMeterTypeHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::LifeBarOptions,
        behavior: RowBehavior::Bitmask(LIFE_BAR_OPTIONS),
        name: lookup_key("PlayerOptions", "LifeBarOptions"),
        choices: vec![
            tr("PlayerOptions", "LifeBarOptionsRainbowMax").to_string(),
            tr("PlayerOptions", "LifeBarOptionsResponsiveColors").to_string(),
            tr("PlayerOptions", "LifeBarOptionsShowLifePercentage").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "LifeBarOptionsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::DataVisualizations,
        behavior: RowBehavior::Cycle(CycleBinding::Index(DATA_VISUALIZATIONS)),
        name: lookup_key("PlayerOptions", "DataVisualizations"),
        choices: vec![
            tr("PlayerOptions", "DataVisualizationsNone").to_string(),
            tr("PlayerOptions", "DataVisualizationsTargetScoreGraph").to_string(),
            tr("PlayerOptions", "DataVisualizationsStepStatistics").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "DataVisualizationsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::DensityGraphBackground,
        behavior: RowBehavior::Cycle(CycleBinding::Bool(DENSITY_GRAPH_BACKGROUND)),
        name: lookup_key("PlayerOptions", "DensityGraphBackground"),
        choices: vec![
            tr("PlayerOptions", "DensityGraphBackgroundSolid").to_string(),
            tr("PlayerOptions", "DensityGraphBackgroundTransparent").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "DensityGraphBackgroundHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::TargetScore,
        behavior: RowBehavior::Cycle(CycleBinding::Index(TARGET_SCORE)),
        name: lookup_key("PlayerOptions", "TargetScore"),
        choices: vec![
            tr("PlayerOptions", "TargetScoreCMinus").to_string(),
            tr("PlayerOptions", "TargetScoreC").to_string(),
            tr("PlayerOptions", "TargetScoreCPlus").to_string(),
            tr("PlayerOptions", "TargetScoreBMinus").to_string(),
            tr("PlayerOptions", "TargetScoreB").to_string(),
            tr("PlayerOptions", "TargetScoreBPlus").to_string(),
            tr("PlayerOptions", "TargetScoreAMinus").to_string(),
            tr("PlayerOptions", "TargetScoreA").to_string(),
            tr("PlayerOptions", "TargetScoreAPlus").to_string(),
            tr("PlayerOptions", "TargetScoreSMinus").to_string(),
            tr("PlayerOptions", "TargetScoreS").to_string(),
            tr("PlayerOptions", "TargetScoreSPlus").to_string(),
            tr("PlayerOptions", "TargetScoreMachineBest").to_string(),
            tr("PlayerOptions", "TargetScorePersonalBest").to_string(),
        ],
        selected_choice_index: [10; PLAYER_SLOTS], // S by default
        help: vec![tr("PlayerOptionsHelp", "TargetScoreHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ActionOnMissedTarget,
        behavior: RowBehavior::Custom(ACTION_ON_MISSED_TARGET),
        name: lookup_key("PlayerOptions", "TargetScoreMissPolicy"),
        choices: vec![
            tr("PlayerOptions", "TargetScoreMissPolicyNothing").to_string(),
            tr("PlayerOptions", "TargetScoreMissPolicyFail").to_string(),
            tr("PlayerOptions", "TargetScoreMissPolicyRestartSong").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "TargetScoreMissPolicyHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::MiniIndicator,
        behavior: RowBehavior::Custom(MINI_INDICATOR),
        name: lookup_key("PlayerOptions", "MiniIndicator"),
        choices: vec![
            tr("PlayerOptions", "MiniIndicatorNone").to_string(),
            tr("PlayerOptions", "MiniIndicatorSubtractiveScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPredictiveScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPaceScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorRivalScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPacemaker").to_string(),
            tr("PlayerOptions", "MiniIndicatorStreamProg").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MiniIndicatorHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::IndicatorScoreType,
        behavior: RowBehavior::Cycle(CycleBinding::Index(INDICATOR_SCORE_TYPE)),
        name: lookup_key("PlayerOptions", "IndicatorScoreType"),
        choices: vec![
            tr("PlayerOptions", "IndicatorScoreTypeITG").to_string(),
            tr("PlayerOptions", "IndicatorScoreTypeEX").to_string(),
            tr("PlayerOptions", "IndicatorScoreTypeHEX").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "IndicatorScoreTypeHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::GameplayExtras,
        behavior: RowBehavior::Bitmask(GAMEPLAY_EXTRAS),
        name: lookup_key("PlayerOptions", "GameplayExtras"),
        choices: gameplay_extras_choices,
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "GameplayExtrasHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ComboColors,
        behavior: RowBehavior::Cycle(CycleBinding::Index(COMBO_COLORS)),
        name: lookup_key("PlayerOptions", "ComboColors"),
        choices: vec![
            tr("PlayerOptions", "ComboColorsGlow").to_string(),
            tr("PlayerOptions", "ComboColorsSolid").to_string(),
            tr("PlayerOptions", "ComboColorsRainbow").to_string(),
            tr("PlayerOptions", "ComboColorsRainbowScroll").to_string(),
            tr("PlayerOptions", "ComboColorsNone").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ComboColorsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ComboColorMode,
        behavior: RowBehavior::Cycle(CycleBinding::Index(COMBO_COLOR_MODE)),
        name: lookup_key("PlayerOptions", "ComboColorMode"),
        choices: vec![
            tr("PlayerOptions", "ComboColorModeFullCombo").to_string(),
            tr("PlayerOptions", "ComboColorModeCurrentCombo").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ComboColorModeHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::CarryCombo,
        behavior: RowBehavior::Cycle(CycleBinding::Bool(CARRY_COMBO)),
        name: lookup_key("PlayerOptions", "CarryCombo"),
        choices: vec![
            tr("PlayerOptions", "CarryComboNo").to_string(),
            tr("PlayerOptions", "CarryComboYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "CarryComboHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::JudgmentTilt,
        behavior: RowBehavior::Cycle(CycleBinding::Bool(JUDGMENT_TILT)),
        name: lookup_key("PlayerOptions", "JudgmentTilt"),
        choices: vec![
            tr("PlayerOptions", "JudgmentTiltNo").to_string(),
            tr("PlayerOptions", "JudgmentTiltYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "JudgmentTiltHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::JudgmentTiltIntensity,
        behavior: RowBehavior::Custom(JUDGMENT_TILT_INTENSITY),
        name: lookup_key("PlayerOptions", "JudgmentTiltIntensity"),
        choices: tilt_intensity_choices(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "JudgmentTiltIntensityHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::JudgmentBehindArrows,
        behavior: RowBehavior::Cycle(CycleBinding::Bool(JUDGMENT_BEHIND_ARROWS)),
        name: lookup_key("PlayerOptions", "JudgmentBehindArrows"),
        choices: vec![
            tr("PlayerOptions", "JudgmentBehindArrowsOff").to_string(),
            tr("PlayerOptions", "JudgmentBehindArrowsOn").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "JudgmentBehindArrowsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::OffsetIndicator,
        behavior: RowBehavior::Cycle(CycleBinding::Bool(OFFSET_INDICATOR)),
        name: lookup_key("PlayerOptions", "OffsetIndicator"),
        choices: vec![
            tr("PlayerOptions", "OffsetIndicatorOff").to_string(),
            tr("PlayerOptions", "OffsetIndicatorOn").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "OffsetIndicatorHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ErrorBar,
        behavior: RowBehavior::Bitmask(ERROR_BAR),
        name: lookup_key("PlayerOptions", "ErrorBar"),
        choices: vec![
            tr("PlayerOptions", "ErrorBarColorful").to_string(),
            tr("PlayerOptions", "ErrorBarMonochrome").to_string(),
            tr("PlayerOptions", "ErrorBarText").to_string(),
            tr("PlayerOptions", "ErrorBarHighlight").to_string(),
            tr("PlayerOptions", "ErrorBarAverage").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ErrorBarTrim,
        behavior: RowBehavior::Cycle(CycleBinding::Index(ERROR_BAR_TRIM)),
        name: lookup_key("PlayerOptions", "ErrorBarTrim"),
        choices: vec![
            tr("PlayerOptions", "ErrorBarTrimOff").to_string(),
            tr("PlayerOptions", "ErrorBarTrimFantastic").to_string(),
            tr("PlayerOptions", "ErrorBarTrimExcellent").to_string(),
            tr("PlayerOptions", "ErrorBarTrimGreat").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarTrimHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ErrorBarOptions,
        behavior: RowBehavior::Bitmask(ERROR_BAR_OPTIONS),
        name: lookup_key("PlayerOptions", "ErrorBarOptions"),
        choices: vec![
            tr("PlayerOptions", "ErrorBarOptionsMoveUp").to_string(),
            tr("PlayerOptions", "ErrorBarOptionsMultiTick").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarOptionsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ErrorBarOffsetX,
        behavior: RowBehavior::Numeric(ERROR_BAR_OFFSET_X),
        name: lookup_key("PlayerOptions", "ErrorBarOffsetX"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarOffsetXHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ErrorBarOffsetY,
        behavior: RowBehavior::Numeric(ERROR_BAR_OFFSET_Y),
        name: lookup_key("PlayerOptions", "ErrorBarOffsetY"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarOffsetYHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::MeasureCounter,
        behavior: RowBehavior::Cycle(CycleBinding::Index(MEASURE_COUNTER)),
        name: lookup_key("PlayerOptions", "MeasureCounter"),
        choices: vec![
            tr("PlayerOptions", "MeasureCounterNone").to_string(),
            tr("PlayerOptions", "MeasureCounter8th").to_string(),
            tr("PlayerOptions", "MeasureCounter12th").to_string(),
            tr("PlayerOptions", "MeasureCounter16th").to_string(),
            tr("PlayerOptions", "MeasureCounter24th").to_string(),
            tr("PlayerOptions", "MeasureCounter32nd").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureCounterHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::MeasureCounterLookahead,
        behavior: RowBehavior::Custom(MEASURE_COUNTER_LOOKAHEAD),
        name: lookup_key("PlayerOptions", "MeasureCounterLookahead"),
        choices: vec![
            tr("PlayerOptions", "MeasureCounterLookahead0").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead1").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead2").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead3").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead4").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureCounterLookaheadHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::MeasureCounterOptions,
        behavior: RowBehavior::Bitmask(MEASURE_COUNTER_OPTIONS),
        name: lookup_key("PlayerOptions", "MeasureCounterOptions"),
        choices: vec![
            tr("PlayerOptions", "MeasureCounterOptionsMoveLeft").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsMoveUp").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsVerticalLookahead").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsBrokenRunTotal").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsRunTimer").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureCounterOptionsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::MeasureLines,
        behavior: RowBehavior::Cycle(CycleBinding::Index(MEASURE_LINES)),
        name: lookup_key("PlayerOptions", "MeasureLines"),
        choices: vec![
            tr("PlayerOptions", "MeasureLinesOff").to_string(),
            tr("PlayerOptions", "MeasureLinesMeasure").to_string(),
            tr("PlayerOptions", "MeasureLinesQuarter").to_string(),
            tr("PlayerOptions", "MeasureLinesEighth").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureLinesHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::RescoreEarlyHits,
        behavior: RowBehavior::Cycle(CycleBinding::Bool(RESCORE_EARLY_HITS)),
        name: lookup_key("PlayerOptions", "RescoreEarlyHits"),
        choices: vec![
            tr("PlayerOptions", "RescoreEarlyHitsNo").to_string(),
            tr("PlayerOptions", "RescoreEarlyHitsYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "RescoreEarlyHitsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::EarlyDecentWayOffOptions,
        behavior: RowBehavior::Bitmask(EARLY_DW_OPTIONS),
        name: lookup_key("PlayerOptions", "EarlyDecentWayOffOptions"),
        choices: vec![
            tr("PlayerOptions", "EarlyDecentWayOffOptionsHideJudgments").to_string(),
            tr(
                "PlayerOptions",
                "EarlyDecentWayOffOptionsHideNoteFieldFlash",
            )
            .to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "EarlyDecentWayOffOptionsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ResultsExtras,
        behavior: RowBehavior::Bitmask(RESULTS_EXTRAS),
        name: lookup_key("PlayerOptions", "ResultsExtras"),
        choices: vec![tr("PlayerOptions", "ResultsExtrasTrackEarlyJudgments").to_string()],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ResultsExtrasHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::TimingWindows,
        behavior: RowBehavior::Cycle(CycleBinding::Index(TIMING_WINDOWS)),
        name: lookup_key("PlayerOptions", "TimingWindows"),
        choices: vec![
            tr("PlayerOptions", "TimingWindowsNone").to_string(),
            tr("PlayerOptions", "TimingWindowsWayOffs").to_string(),
            tr("PlayerOptions", "TimingWindowsDecentsAndWayOffs").to_string(),
            tr("PlayerOptions", "TimingWindowsFantasticsAndExcellents").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "TimingWindowsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::FAPlusOptions,
        behavior: RowBehavior::Bitmask(FA_PLUS_OPTIONS),
        name: lookup_key("PlayerOptions", "FAPlusOptions"),
        choices: vec![
            tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusWindow").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayEXScore").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayHEXScore").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusPane").to_string(),
            tr("PlayerOptions", "FAPlusOptions10msBlueWindow").to_string(),
            tr("PlayerOptions", "FAPlusOptions1510msSplit").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "FAPlusOptionsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::CustomBlueFantasticWindow,
        behavior: RowBehavior::Cycle(CycleBinding::Bool(CUSTOM_BLUE_FANTASTIC_WINDOW)),
        name: lookup_key("PlayerOptions", "CustomBlueFantasticWindow"),
        choices: vec![
            tr("PlayerOptions", "CustomBlueFantasticWindowNo").to_string(),
            tr("PlayerOptions", "CustomBlueFantasticWindowYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "CustomBlueFantasticWindowHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::CustomBlueFantasticWindowMs,
        behavior: RowBehavior::Custom(CUSTOM_BLUE_FANTASTIC_WINDOW_MS),
        name: lookup_key("PlayerOptions", "CustomBlueFantasticWindowMs"),
        choices: custom_fantastic_window_choices(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "CustomBlueFantasticWindowMsHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::WhatComesNext,
        behavior: RowBehavior::Custom(super::WHAT_COMES_NEXT),
        name: lookup_key("PlayerOptions", "WhatComesNext"),
        choices: what_comes_next_choices(OptionsPane::Advanced, return_screen),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "WhatComesNextAdvancedHelp").to_string()],
        choice_difficulty_indices: None,
        mirror_across_players: true,
    });
    b.push(Row {
        id: RowId::Exit,
        behavior: RowBehavior::Exit,
        name: lookup_key("Common", "Exit"),
        choices: vec![tr("Common", "Exit").to_string()],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![String::new()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.finish()
}

#[cfg(test)]
mod bitmask_binding_init_tests {
    use super::super::super::row::{Row, RowBehavior, RowId, init_bitmask_row_from_binding};
    use super::super::super::state::{FaPlusMask, HideMask, PlayerOptionMasks};
    use super::*;
    use crate::assets::i18n::{LookupKey, lookup_key};
    use crate::game::profile::Profile;

    fn ensure_i18n() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            crate::assets::i18n::init("en");
        });
    }

    fn make_bitmask_row(id: RowId, name: LookupKey, choices: &[&str]) -> Row {
        Row {
            id,
            behavior: RowBehavior::Bitmask(BitmaskBinding {
                toggle: |_, _| {},
                init: None,
            }),
            name,
            choices: choices.iter().map(ToString::to_string).collect(),
            selected_choice_index: [0, 0],
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        }
    }

    /// HIDE binding's data-driven init must reproduce the bits and cursor
    /// that the legacy `apply_profile_defaults` path produces for the same
    /// profile.
    #[test]
    fn hide_binding_init_matches_legacy_path() {
        ensure_i18n();
        let mut profile = Profile::default();
        profile.hide_targets = false;
        profile.hide_song_bg = true;
        profile.hide_combo = true;

        let mut row = make_bitmask_row(
            RowId::Hide,
            lookup_key("PlayerOptions", "Hide"),
            &[
                "Targets", "BG", "Combo", "Life", "Score", "Danger", "ComboExp",
            ],
        );
        let mut masks = PlayerOptionMasks::default();
        let applied = init_bitmask_row_from_binding(&mut row, &HIDE, &profile, &mut masks, 0);
        assert!(applied, "HIDE binding has init contract");
        assert_eq!(
            masks.hide,
            HideMask::BACKGROUND | HideMask::COMBO,
            "data-driven HIDE bits match profile",
        );
        assert_eq!(
            row.selected_choice_index[0], 1,
            "FirstActiveBit cursor lands on BACKGROUND (index 1)",
        );
    }

    /// FA_PLUS_OPTIONS binding's data-driven init must populate the bits
    /// AND pin the cursor to 0 even when a non-first bit is the only one
    /// set (Pattern E: cursor=Fixed(0)).
    #[test]
    fn fa_plus_binding_init_pins_cursor_to_zero() {
        ensure_i18n();
        let mut profile = Profile::default();
        profile.show_fa_plus_window = false;
        profile.show_ex_score = true;

        let mut row = make_bitmask_row(
            RowId::FAPlusOptions,
            lookup_key("PlayerOptions", "FAPlusOptions"),
            &["Window", "EX", "HardEX", "Pane", "Blue10", "Split"],
        );
        let mut masks = PlayerOptionMasks::default();
        let applied =
            init_bitmask_row_from_binding(&mut row, &FA_PLUS_OPTIONS, &profile, &mut masks, 0);
        assert!(applied, "FA_PLUS_OPTIONS binding has init contract");
        assert_eq!(
            masks.fa_plus,
            FaPlusMask::EX_SCORE,
            "data-driven FA+ bits match profile",
        );
        assert_eq!(
            row.selected_choice_index[0], 0,
            "Fixed(0) cursor pins to 0 even though EX_SCORE is the only active bit",
        );
    }

    /// Bindings without an init contract must report `false` and leave
    /// row/masks untouched. Now that all production bindings opt in, this
    /// guard uses a synthetic init-less binding to assert the contract
    /// shape directly.
    #[test]
    fn binding_without_init_is_noop() {
        ensure_i18n();
        let mut row = make_bitmask_row(
            RowId::GameplayExtras,
            lookup_key("PlayerOptions", "GameplayExtras"),
            &["FlashMiss", "DensityTop", "ColumnCues", "Scorebox"],
        );
        row.selected_choice_index = [3, 4];
        let mut masks = PlayerOptionMasks::default();
        let profile = Profile::default();
        let init_less = BitmaskBinding {
            toggle: |_, _| {},
            init: None,
        };
        let applied = init_bitmask_row_from_binding(&mut row, &init_less, &profile, &mut masks, 0);
        assert!(!applied, "init-less binding must short-circuit");
        assert_eq!(row.selected_choice_index, [3, 4], "row untouched");
        assert_eq!(masks, PlayerOptionMasks::default(), "masks untouched");
    }

    /// Order assertion: Scroll choice index N maps to ScrollMask bit (1 << N)
    /// maps to ScrollOption variant N. The SCROLL binding's from_profile
    /// closure relies on this 1:1 ordering; if any of the three orderings
    /// drifts, this test must fail before reaching production.
    #[test]
    fn scroll_choice_order_matches_scroll_option_bits() {
        use crate::game::profile::ScrollOption;
        let cases = [
            (0u8, ScrollOption::Reverse),
            (1, ScrollOption::Split),
            (2, ScrollOption::Alternate),
            (3, ScrollOption::Cross),
            (4, ScrollOption::Centered),
        ];
        for (idx, opt) in cases {
            let mut profile = Profile::default();
            profile.scroll_option = opt;
            let mut row = make_bitmask_row(
                RowId::Scroll,
                lookup_key("PlayerOptions", "Scroll"),
                &["Reverse", "Split", "Alternate", "Cross", "Centered"],
            );
            let mut masks = PlayerOptionMasks::default();
            let applied = init_bitmask_row_from_binding(&mut row, &SCROLL, &profile, &mut masks, 0);
            assert!(applied, "SCROLL binding has init contract");
            assert_eq!(
                masks.scroll.bits(),
                1u8 << idx,
                "ScrollOption variant at choice index {idx} must set bit (1 << {idx})",
            );
            assert_eq!(
                row.selected_choice_index[0], idx as usize,
                "FirstActiveBit cursor lands on choice index {idx}",
            );
        }
    }
}
