use super::super::choice;
use super::super::constants::{
    COLUMN_FLASH_BRIGHTNESS_VARIANTS, COLUMN_FLASH_SIZE_VARIANTS, MINI_INDICATOR_COLOR_VARIANTS,
    MINI_INDICATOR_POSITION_VARIANTS, MINI_INDICATOR_SIZE_VARIANTS,
    MINI_INDICATOR_SUBTRACTIVE_DISPLAY_VARIANTS, MINI_INDICATOR_VARIANTS,
    SCORE_DISPLAY_MODE_VARIANTS, SCORE_POSITION_VARIANTS, STEP_STATS_EXTRA_VARIANTS,
};
use super::super::row::{
    BitMapping, BitmaskInit, BitmaskWriteback, CursorInit, CycleInit, NumericInit,
};
use super::super::row::{fanout_bitmask_binding, index_binding};
use super::super::state::{
    ColumnFlashMask, EarlyDwMask, ErrorBarOptionsMask, FaPlusMask, GameplayExtrasMask,
    GameplayExtrasMoreMask, HideMask, LifeBarOptionsMask, LiveTimingStatsMask,
    MeasureCounterOptionsMask, PlayerOptionMasks, ResultsExtrasMask, STEP_STATISTICS_ROW_WIDTH,
    ScrollMask, step_statistics_choice_bits, step_statistics_mask_from_choice_bits,
};
use super::*;
use deadsync_profile::{
    ColumnFlashBrightness, ColumnFlashSize, ComboColors, ComboMode, ErrorBarMask, ErrorBarTrim,
    LifeMeterType, MeasureCounter, MeasureLines, MiniIndicator, MiniIndicatorColor,
    MiniIndicatorPosition, MiniIndicatorScoreType, MiniIndicatorSize,
    MiniIndicatorSubtractiveDisplay, PlayerOptionsData, ScatterplotMaxWindow, ScoreDisplayMode,
    ScorePosition, StepStatsExtra, TargetScoreSetting, TimingWindowsOption, TurnOption,
};

// =============================== Bindings ===============================

const TURN: ChoiceBinding<usize> = index_binding!(
    TURN_OPTION_VARIANTS,
    TurnOption::None,
    turn_option,
    false,
    Some(CycleInit {
        from_profile: |p| {
            TURN_OPTION_VARIANTS
                .iter()
                .position(|&v| v == p.turn_option)
                .unwrap_or(0)
        }
    })
);
const LIFE_METER_TYPE: ChoiceBinding<usize> = index_binding!(
    LIFE_METER_TYPE_VARIANTS,
    LifeMeterType::Standard,
    lifemeter_type,
    false,
    Some(CycleInit {
        from_profile: |p| {
            LIFE_METER_TYPE_VARIANTS
                .iter()
                .position(|&v| v == p.lifemeter_type)
                .unwrap_or(0)
        }
    })
);
const SCATTERPLOT_MAX_WINDOW: ChoiceBinding<usize> = index_binding!(
    SCATTERPLOT_MAX_WINDOW_VARIANTS,
    ScatterplotMaxWindow::Off,
    scatterplot_max_window,
    false,
    Some(CycleInit {
        from_profile: |p| {
            SCATTERPLOT_MAX_WINDOW_VARIANTS
                .iter()
                .position(|&v| v == p.scatterplot_max_window)
                .unwrap_or(0)
        }
    })
);
const SCORE_POSITION: ChoiceBinding<usize> = index_binding!(
    SCORE_POSITION_VARIANTS,
    ScorePosition::Normal,
    score_position,
    false,
    Some(CycleInit {
        from_profile: |p| {
            SCORE_POSITION_VARIANTS
                .iter()
                .position(|&v| v == p.score_position)
                .unwrap_or(0)
        }
    })
);
const SCORE_DISPLAY_MODE: ChoiceBinding<usize> = index_binding!(
    SCORE_DISPLAY_MODE_VARIANTS,
    ScoreDisplayMode::Normal,
    score_display_mode,
    false,
    Some(CycleInit {
        from_profile: |p| {
            SCORE_DISPLAY_MODE_VARIANTS
                .iter()
                .position(|&v| v == p.score_display_mode)
                .unwrap_or(0)
        }
    })
);
const STEP_STATS_EXTRA: ChoiceBinding<usize> = index_binding!(
    STEP_STATS_EXTRA_VARIANTS,
    StepStatsExtra::None,
    step_stats_extra,
    false,
    Some(CycleInit {
        from_profile: |p| {
            STEP_STATS_EXTRA_VARIANTS
                .iter()
                .position(|&v| v == p.step_stats_extra)
                .unwrap_or(0)
        }
    })
);
const TARGET_SCORE: ChoiceBinding<usize> = index_binding!(
    TARGET_SCORE_VARIANTS,
    TargetScoreSetting::S,
    target_score,
    false,
    Some(CycleInit {
        from_profile: |p| {
            TARGET_SCORE_VARIANTS
                .iter()
                .position(|&v| v == p.target_score)
                .unwrap_or(0)
        }
    })
);
const INDICATOR_SCORE_TYPE: ChoiceBinding<usize> = index_binding!(
    MINI_INDICATOR_SCORE_TYPE_VARIANTS,
    MiniIndicatorScoreType::Itg,
    mini_indicator_score_type,
    false,
    Some(CycleInit {
        from_profile: |p| {
            MINI_INDICATOR_SCORE_TYPE_VARIANTS
                .iter()
                .position(|&v| v == p.mini_indicator_score_type)
                .unwrap_or(0)
        }
    })
);
const MINI_INDICATOR_SUBTRACTIVE_DISPLAY: ChoiceBinding<usize> = index_binding!(
    MINI_INDICATOR_SUBTRACTIVE_DISPLAY_VARIANTS,
    MiniIndicatorSubtractiveDisplay::Percent,
    mini_indicator_subtractive_display,
    false,
    Some(CycleInit {
        from_profile: |p| {
            MINI_INDICATOR_SUBTRACTIVE_DISPLAY_VARIANTS
                .iter()
                .position(|&v| v == p.mini_indicator_subtractive_display)
                .unwrap_or(0)
        }
    })
);
const MINI_INDICATOR_SIZE: ChoiceBinding<usize> = index_binding!(
    MINI_INDICATOR_SIZE_VARIANTS,
    MiniIndicatorSize::Default,
    mini_indicator_size,
    false,
    Some(CycleInit {
        from_profile: |p| {
            MINI_INDICATOR_SIZE_VARIANTS
                .iter()
                .position(|&v| v == p.mini_indicator_size)
                .unwrap_or(0)
        }
    })
);
const MINI_INDICATOR_COLOR: ChoiceBinding<usize> = index_binding!(
    MINI_INDICATOR_COLOR_VARIANTS,
    MiniIndicatorColor::Default,
    mini_indicator_color,
    false,
    Some(CycleInit {
        from_profile: |p| {
            MINI_INDICATOR_COLOR_VARIANTS
                .iter()
                .position(|&v| v == p.mini_indicator_color)
                .unwrap_or(0)
        }
    })
);
const MINI_INDICATOR_POSITION: ChoiceBinding<usize> = index_binding!(
    MINI_INDICATOR_POSITION_VARIANTS,
    MiniIndicatorPosition::Default,
    mini_indicator_position,
    false,
    Some(CycleInit {
        from_profile: |p| {
            MINI_INDICATOR_POSITION_VARIANTS
                .iter()
                .position(|&v| v == p.mini_indicator_position)
                .unwrap_or(0)
        }
    })
);
const COMBO_COLORS: ChoiceBinding<usize> = index_binding!(
    COMBO_COLORS_VARIANTS,
    ComboColors::Glow,
    combo_colors,
    false,
    Some(CycleInit {
        from_profile: |p| {
            COMBO_COLORS_VARIANTS
                .iter()
                .position(|&v| v == p.combo_colors)
                .unwrap_or(0)
        }
    })
);
const COMBO_COLOR_MODE: ChoiceBinding<usize> = index_binding!(
    COMBO_MODE_VARIANTS,
    ComboMode::FullCombo,
    combo_mode,
    false,
    Some(CycleInit {
        from_profile: |p| {
            COMBO_MODE_VARIANTS
                .iter()
                .position(|&v| v == p.combo_mode)
                .unwrap_or(0)
        }
    })
);
const ERROR_BAR_TRIM: ChoiceBinding<usize> = index_binding!(
    ERROR_BAR_TRIM_VARIANTS,
    ErrorBarTrim::Off,
    error_bar_trim,
    false,
    Some(CycleInit {
        from_profile: |p| {
            ERROR_BAR_TRIM_VARIANTS
                .iter()
                .position(|&v| v == p.error_bar_trim)
                .unwrap_or(0)
        }
    })
);
const MEASURE_COUNTER: ChoiceBinding<usize> = index_binding!(
    MEASURE_COUNTER_VARIANTS,
    MeasureCounter::None,
    measure_counter,
    true,
    Some(CycleInit {
        from_profile: |p| {
            MEASURE_COUNTER_VARIANTS
                .iter()
                .position(|&v| v == p.measure_counter)
                .unwrap_or(0)
        }
    })
);
const MEASURE_LINES: ChoiceBinding<usize> = index_binding!(
    MEASURE_LINES_VARIANTS,
    MeasureLines::Off,
    measure_lines,
    false,
    Some(CycleInit {
        from_profile: |p| {
            MEASURE_LINES_VARIANTS
                .iter()
                .position(|&v| v == p.measure_lines)
                .unwrap_or(0)
        }
    })
);
const TIMING_WINDOWS: ChoiceBinding<usize> = index_binding!(
    TIMING_WINDOWS_VARIANTS,
    TimingWindowsOption::None,
    timing_windows,
    false,
    Some(CycleInit {
        from_profile: |p| {
            TIMING_WINDOWS_VARIANTS
                .iter()
                .position(|&v| v == p.timing_windows)
                .unwrap_or(0)
        }
    })
);

const DENSITY_GRAPH_BACKGROUND: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.transparent_density_graph_bg = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| {
            if p.transparent_density_graph_bg { 1 } else { 0 }
        },
    }),
};
const SMX_FSR_DISPLAY: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.smx_fsr_display = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.smx_fsr_display { 1 } else { 0 },
    }),
};
const SMX_PAD_INPUT_DISPLAY: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.smx_pad_input_display = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.smx_pad_input_display { 1 } else { 0 },
    }),
};
const CARRY_COMBO: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.carry_combo_between_songs = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| {
            if p.carry_combo_between_songs { 1 } else { 0 }
        },
    }),
};
const LONG_ERROR_BAR: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.long_error_bar_enabled = v;
        Outcome::persisted_with_visibility()
    },
    init: Some(CycleInit {
        from_profile: |p| {
            if p.long_error_bar_enabled { 1 } else { 0 }
        },
    }),
};
const SHORT_AVERAGE_ERROR_BAR: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.short_average_error_bar_enabled = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| {
            if p.short_average_error_bar_enabled {
                1
            } else {
                0
            }
        },
    }),
};
const CENTER_TICK: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.center_tick = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.center_tick { 1 } else { 0 },
    }),
};
const TEXT_ERROR_BAR_MODE: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.text_error_bar_scalable = v;
        Outcome::persisted_with_visibility()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.text_error_bar_scalable { 1 } else { 0 },
    }),
};
const JUDGMENT_TILT: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.judgment_tilt = v;
        Outcome::persisted_with_visibility()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.judgment_tilt { 1 } else { 0 },
    }),
};
const JUDGMENT_BEHIND_ARROWS: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.judgment_back = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.judgment_back { 1 } else { 0 },
    }),
};
const OFFSET_INDICATOR: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.error_ms_display = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.error_ms_display { 1 } else { 0 },
    }),
};
const RESCORE_EARLY_HITS: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.rescore_early_hits = v;
        Outcome::persisted_with_visibility()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.rescore_early_hits { 1 } else { 0 },
    }),
};
const CUSTOM_BLUE_FANTASTIC_WINDOW: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.custom_fantastic_window = v;
        Outcome::persisted_with_visibility()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.custom_fantastic_window { 1 } else { 0 },
    }),
};
const CROSSOVER_CUES: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.crossover_cues = v;
        Outcome::persisted_with_visibility()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.crossover_cues { 1 } else { 0 },
    }),
};
const CROSSOVER_CUE_BRACKETS: ChoiceBinding<bool> = ChoiceBinding::<bool> {
    apply: |p, v| {
        p.crossover_cue_brackets = v;
        Outcome::persisted()
    },
    init: Some(CycleInit {
        from_profile: |p| if p.crossover_cue_brackets { 1 } else { 0 },
    }),
};

const ERROR_BAR_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.error_bar_offset_x = v;
        Outcome::persisted()
    },
    init: Some(NumericInit {
        from_profile: |p| p.error_bar_offset_x.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        format: |v| format!("{v}"),
    }),
};
const ERROR_BAR_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.error_bar_offset_y = v;
        Outcome::persisted()
    },
    init: Some(NumericInit {
        from_profile: |p| p.error_bar_offset_y.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        format: |v| format!("{v}"),
    }),
};

const SCROLL: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| {
            // The Scroll row's choice indices are fixed by build order
            // (0=Reverse, 1=Split, 2=Alternate, 3=Cross, 4=Centered) and the
            // ScrollOption bit positions match. The order assertion in
            // ``scroll_choice_order_matches_scroll_option_bits`` guards the
            // invariant. We translate per-flag rather than copying ``.0`` so
            // any future divergence is caught here.
            use deadsync_profile::ScrollOption;
            let mut bits = ScrollMask::empty();
            if p.scroll_option.contains(ScrollOption::Reverse) {
                bits.insert(ScrollMask::from_bits_retain(1u8 << 0));
            }
            if p.scroll_option.contains(ScrollOption::Split) {
                bits.insert(ScrollMask::from_bits_retain(1u8 << 1));
            }
            if p.scroll_option.contains(ScrollOption::Alternate) {
                bits.insert(ScrollMask::from_bits_retain(1u8 << 2));
            }
            if p.scroll_option.contains(ScrollOption::Cross) {
                bits.insert(ScrollMask::from_bits_retain(1u8 << 3));
            }
            if p.scroll_option.contains(ScrollOption::Centered) {
                bits.insert(ScrollMask::from_bits_retain(1u8 << 4));
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
            m.scroll = ScrollMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |_m, p, b| {
            use deadsync_profile::ScrollOption;
            let mask = ScrollMask::from_bits_truncate(b as u8);
            let mut setting = ScrollOption::Normal;
            if mask.contains(ScrollMask::REVERSE) {
                setting = setting.union(ScrollOption::Reverse);
            }
            if mask.contains(ScrollMask::SPLIT) {
                setting = setting.union(ScrollOption::Split);
            }
            if mask.contains(ScrollMask::ALTERNATE) {
                setting = setting.union(ScrollOption::Alternate);
            }
            if mask.contains(ScrollMask::CROSS) {
                setting = setting.union(ScrollOption::Cross);
            }
            if mask.contains(ScrollMask::CENTERED) {
                setting = setting.union(ScrollOption::Centered);
            }
            p.scroll_option = setting;
            p.reverse_scroll = setting.contains(ScrollOption::Reverse);
        },
        bit_mapping: BitMapping::Sequential { width: 5 },
        sync_visibility: false,
    },
};
const HIDE: BitmaskBinding = fanout_bitmask_binding!(
    mask = HideMask,
    bits = u8,
    state_field = hide,
    fields = [
        (TARGETS, hide_targets),
        (BACKGROUND, hide_song_bg),
        (COMBO, hide_combo),
        (LIFE, hide_lifebar),
        (SCORE, hide_score),
        (DANGER, hide_danger),
        (COMBO_EXPLOSIONS, hide_combo_explosions),
        (USERNAME, hide_username),
    ],
    sync_visibility = true,
);
const LIFE_BAR_OPTIONS: BitmaskBinding = fanout_bitmask_binding!(
    mask = LifeBarOptionsMask,
    bits = u8,
    state_field = life_bar_options,
    fields = [
        (RAINBOW_MAX, rainbow_max),
        (RESPONSIVE_COLORS, responsive_colors),
        (SHOW_LIFE_PERCENT, show_life_percent),
    ],
    sync_visibility = false,
);
const STEP_STATISTICS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| step_statistics_choice_bits(p.step_statistics) as u32,
        get_active: |m| step_statistics_choice_bits(m.step_statistics) as u32,
        set_active: |m, b| {
            m.step_statistics = step_statistics_mask_from_choice_bits(b);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |m, p, b| {
            let mask = step_statistics_mask_from_choice_bits(b);
            p.step_statistics = mask;
            m.step_statistics = mask;
        },
        bit_mapping: BitMapping::Sequential {
            width: STEP_STATISTICS_ROW_WIDTH,
        },
        sync_visibility: true,
    },
};
const GAMEPLAY_EXTRAS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| {
            let mut bits = GameplayExtrasMask::empty();
            if p.column_flash_on_miss {
                bits.insert(GameplayExtrasMask::FLASH_COLUMN_FOR_MISS);
            }
            if p.nps_graph_at_top {
                bits.insert(GameplayExtrasMask::DENSITY_GRAPH_AT_TOP);
            }
            if p.column_cues {
                bits.insert(GameplayExtrasMask::COLUMN_CUES);
            }
            if p.measure_cues {
                bits.insert(GameplayExtrasMask::MEASURE_CUES);
            }
            if p.live_timing_stats {
                bits.insert(GameplayExtrasMask::LIVE_TIMING_STATS);
            }
            if p.column_countdown {
                bits.insert(GameplayExtrasMask::COLUMN_COUNTDOWN);
            }
            if p.display_scorebox {
                bits.insert(GameplayExtrasMask::DISPLAY_SCOREBOX);
            }
            bits.bits() as u32
        },
        get_active: |m| m.gameplay_extras.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u16::MAX as u32),
                0,
                "GameplayExtrasMask init bits exceed u16 width",
            );
            m.gameplay_extras = GameplayExtrasMask::from_bits_retain(b as u16);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |m, p, b| {
            let mask = GameplayExtrasMask::from_bits_truncate(b as u16);
            p.column_flash_on_miss = mask.contains(GameplayExtrasMask::FLASH_COLUMN_FOR_MISS);
            p.nps_graph_at_top = mask.contains(GameplayExtrasMask::DENSITY_GRAPH_AT_TOP);
            p.column_cues = mask.contains(GameplayExtrasMask::COLUMN_CUES);
            p.measure_cues = mask.contains(GameplayExtrasMask::MEASURE_CUES);
            p.live_timing_stats = mask.contains(GameplayExtrasMask::LIVE_TIMING_STATS);
            p.column_countdown = mask.contains(GameplayExtrasMask::COLUMN_COUNTDOWN);
            p.display_scorebox = mask.contains(GameplayExtrasMask::DISPLAY_SCOREBOX);
            let mut more = GameplayExtrasMoreMask::empty();
            if p.column_cues {
                more.insert(GameplayExtrasMoreMask::COLUMN_CUES);
            }
            if p.display_scorebox {
                more.insert(GameplayExtrasMoreMask::DISPLAY_SCOREBOX);
            }
            m.gameplay_extras_more = more;
        },
        bit_mapping: BitMapping::Sequential { width: 7 },
        sync_visibility: true,
    },
};
const COLUMN_FLASH_JUDGMENTS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.column_flash_mask.bits() as u32,
        get_active: |m| m.column_flash.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "ColumnFlashMask init bits exceed u8 width",
            );
            m.column_flash = ColumnFlashMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |m, p, b| {
            let mask = ColumnFlashMask::from_bits_truncate(b as u8);
            p.column_flash_mask = mask;
            m.column_flash = mask;
        },
        bit_mapping: BitMapping::Sequential { width: 7 },
        sync_visibility: false,
    },
};
const COLUMN_FLASH_BRIGHTNESS: ChoiceBinding<usize> = index_binding!(
    COLUMN_FLASH_BRIGHTNESS_VARIANTS,
    ColumnFlashBrightness::Normal,
    column_flash_brightness,
    false,
    Some(CycleInit {
        from_profile: |p| {
            COLUMN_FLASH_BRIGHTNESS_VARIANTS
                .iter()
                .position(|&v| v == p.column_flash_brightness)
                .unwrap_or(0)
        }
    })
);
const COLUMN_FLASH_SIZE: ChoiceBinding<usize> = index_binding!(
    COLUMN_FLASH_SIZE_VARIANTS,
    ColumnFlashSize::Default,
    column_flash_size,
    false,
    Some(CycleInit {
        from_profile: |p| {
            COLUMN_FLASH_SIZE_VARIANTS
                .iter()
                .position(|&v| v == p.column_flash_size)
                .unwrap_or(0)
        }
    })
);
const LIVE_TIMING_STATS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.live_timing_stats_mask.bits() as u32,
        get_active: |m| m.live_timing_stats.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "LiveTimingStatsMask init bits exceed u8 width",
            );
            m.live_timing_stats = LiveTimingStatsMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |m, p, b| {
            let mask = LiveTimingStatsMask::from_bits_truncate(b as u8);
            p.live_timing_stats_mask = mask;
            m.live_timing_stats = mask;
        },
        bit_mapping: BitMapping::Sequential { width: 3 },
        sync_visibility: false,
    },
};
const ERROR_BAR: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| {
            // PlayerOptionsData already stores the desired mask; if it's empty (e.g.
            // legacy profile or unset) fall back to the canonical mapping
            // from the visual style + text-mode pair.
            let mask = if p.error_bar_active_mask.is_empty() {
                deadsync_profile::error_bar_mask_from_style(p.error_bar, p.error_bar_text)
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
            m.error_bar = ErrorBarMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |_m, p, b| {
            let mask = ErrorBarMask::from_bits_truncate(b as u8);
            p.error_bar_active_mask = mask;
            p.error_bar = deadsync_profile::error_bar_style_from_mask(mask);
            p.error_bar_text = deadsync_profile::error_bar_text_from_mask(mask);
        },
        bit_mapping: BitMapping::Sequential { width: 5 },
        sync_visibility: true,
    },
};
const ERROR_BAR_OPTIONS: BitmaskBinding = fanout_bitmask_binding!(
    mask = ErrorBarOptionsMask,
    bits = u8,
    state_field = error_bar_options,
    fields = [(MOVE_UP, error_bar_up), (MULTI_TICK, error_bar_multi_tick),],
    sync_visibility = false,
);
const MEASURE_COUNTER_OPTIONS: BitmaskBinding = fanout_bitmask_binding!(
    mask = MeasureCounterOptionsMask,
    bits = u8,
    state_field = measure_counter_options,
    fields = [
        (MOVE_LEFT, measure_counter_left),
        (MOVE_UP, measure_counter_up),
        (VERTICAL_LOOKAHEAD, measure_counter_vert),
        (BROKEN_RUN_TOTAL, broken_run),
        (RUN_TIMER, run_timer),
    ],
    sync_visibility = false,
);
fn fa_plus_bits_from_profile(p: &PlayerOptionsData) -> u32 {
    let mut bits = FaPlusMask::empty();
    if p.show_fa_plus_window {
        bits.insert(FaPlusMask::WINDOW);
    }
    if p.show_ex_score {
        bits.insert(FaPlusMask::EX_SCORE);
    }
    if p.show_hard_ex_score {
        bits.insert(FaPlusMask::HARD_EX_SCORE);
    }
    if p.show_fa_plus_pane {
        bits.insert(FaPlusMask::PANE);
    }
    if p.fa_plus_10ms_blue_window {
        bits.insert(FaPlusMask::BLUE_WINDOW_10MS);
    }
    if p.split_15_10ms {
        bits.insert(FaPlusMask::SPLIT_15_10MS);
    }
    bits.bits() as u32
}

/// Project the full FA+ mask onto every fan-out profile field. Both FA+
/// rows share the same projection — they only differ in which slice of
/// `FaPlusMask` they own at toggle time.
fn project_fa_plus(
    _m: &mut PlayerOptionMasks,
    p: &mut PlayerOptionsData,
    _b: u32,
    mask: FaPlusMask,
) {
    p.show_fa_plus_window = mask.contains(FaPlusMask::WINDOW);
    p.show_ex_score = mask.contains(FaPlusMask::EX_SCORE);
    p.show_hard_ex_score = mask.contains(FaPlusMask::HARD_EX_SCORE);
    p.show_fa_plus_pane = mask.contains(FaPlusMask::PANE);
    p.fa_plus_10ms_blue_window = mask.contains(FaPlusMask::BLUE_WINDOW_10MS);
    p.split_15_10ms = mask.contains(FaPlusMask::SPLIT_15_10MS);
}

const FA_PLUS_OPTIONS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        // FA+ Options owns bits 0..=3 of FaPlusMask (WINDOW/EX/HARD_EX/PANE).
        // The row stores its slice in row-local coordinates: choice i ⇔ bit i.
        from_profile: |p| (fa_plus_bits_from_profile(p)) & 0b0000_1111,
        get_active: |m| (m.fa_plus.bits() & 0b0000_1111) as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !0b0000_1111u32,
                0,
                "FA+ Options bits exceed slice width"
            );
            let preserved = m.fa_plus.bits() & 0b1111_0000;
            m.fa_plus = FaPlusMask::from_bits_retain(preserved | (b as u8 & 0b0000_1111));
        },
        cursor: CursorInit::Fixed(0),
    },
    writeback: BitmaskWriteback {
        project: |m, p, _b| {
            let mask = m.fa_plus;
            project_fa_plus(m, p, _b, mask);
        },
        bit_mapping: BitMapping::Sequential { width: 4 },
        // Toggling the WINDOW bit hides/shows FA+ Window Options. We always
        // sync after FA+ Options toggles; sync is cheap and a no-op when
        // nothing changed.
        sync_visibility: true,
    },
};

const FA_PLUS_WINDOW_OPTIONS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        // FA+ Window Options owns bits 4..=5 of FaPlusMask (BLUE_WINDOW_10MS,
        // SPLIT_15_10MS), shifted into row-local 0..=1.
        from_profile: |p| (fa_plus_bits_from_profile(p) >> 4) & 0b0000_0011,
        get_active: |m| ((m.fa_plus.bits() >> 4) & 0b0000_0011) as u32,
        set_active: |m, b| {
            debug_assert_eq!(b & !0b0000_0011u32, 0, "FA+ Window bits exceed slice width");
            let preserved = m.fa_plus.bits() & 0b1100_1111;
            m.fa_plus = FaPlusMask::from_bits_retain(preserved | (((b as u8) & 0b11) << 4));
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |m, p, _b| {
            let mask = m.fa_plus;
            project_fa_plus(m, p, _b, mask);
        },
        bit_mapping: BitMapping::Sequential { width: 2 },
        sync_visibility: false,
    },
};
const EARLY_DW_OPTIONS: BitmaskBinding = fanout_bitmask_binding!(
    mask = EarlyDwMask,
    bits = u8,
    state_field = early_dw,
    fields = [
        (HIDE_JUDGMENTS, hide_early_dw_judgments),
        (HIDE_FLASH, hide_early_dw_flash),
        (HIDE_COLUMN_FLASH, hide_early_dw_column_flash),
    ],
    sync_visibility = false,
);
const RESULTS_EXTRAS: BitmaskBinding = fanout_bitmask_binding!(
    mask = ResultsExtrasMask,
    bits = u8,
    state_field = results_extras,
    fields = [
        (TRACK_EARLY_JUDGMENTS, track_early_judgments),
        (SCALE_SCATTERPLOT, scale_scatterplot),
        (DIM_POST_FAIL_SCATTER, dim_post_fail_scatter),
    ],
    sync_visibility = false,
);

const ACTION_ON_MISSED_TARGET: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        if choice::cycle_choice_index(state, player_idx, row_id, delta, wrap).is_none() {
            return Outcome::NONE;
        }
        Outcome::persisted_with_visibility()
    },
};

const MINI_INDICATOR: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let mini_indicator = MINI_INDICATOR_VARIANTS
            .get(new_index)
            .copied()
            .unwrap_or(MiniIndicator::None);
        let subtractive_scoring = mini_indicator == MiniIndicator::SubtractiveScoring;
        let pacemaker = mini_indicator == MiniIndicator::Pacemaker;
        state.player_options[player_idx].mini_indicator = mini_indicator;
        state.player_options[player_idx].subtractive_scoring = subtractive_scoring;
        state.player_options[player_idx].pacemaker = pacemaker;
        Outcome::persisted_with_visibility()
    },
};

const JUDGMENT_TILT_INTENSITY: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        state.player_options[player_idx].tilt_multiplier = mult;
        Outcome::persisted()
    },
};

const AVERAGE_ERROR_BAR_INTENSITY: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let parsed = choice.trim().trim_end_matches('x').trim().parse::<f32>();
        let Ok(raw) = parsed else {
            return Outcome::persisted();
        };
        let value = deadsync_profile::clamp_average_error_bar_intensity(raw);
        state.player_options[player_idx].average_error_bar_intensity = value;
        Outcome::persisted()
    },
};

const AVERAGE_ERROR_BAR_INTERVAL: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let parsed = choice.trim().trim_end_matches("ms").trim().parse::<u32>();
        let Ok(raw) = parsed else {
            return Outcome::persisted();
        };
        let value = deadsync_profile::clamp_average_error_bar_interval_ms(raw);
        state.player_options[player_idx].average_error_bar_interval_ms = value;
        Outcome::persisted()
    },
};

const TEXT_ERROR_BAR_THRESHOLD: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let Some(value) = parse_text_error_bar_threshold_ms(&choice) else {
            return Outcome::persisted();
        };
        state.player_options[player_idx].text_error_bar_threshold_ms = value;
        Outcome::persisted()
    },
};

const LONG_ERROR_BAR_INTENSITY: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let parsed = choice.trim().trim_end_matches('x').trim().parse::<f32>();
        let Ok(raw) = parsed else {
            return Outcome::persisted();
        };
        let value = deadsync_profile::clamp_long_error_bar_intensity(raw);
        state.player_options[player_idx].long_error_bar_intensity = value;
        Outcome::persisted()
    },
};

const LONG_ERROR_BAR_THRESHOLD: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let parsed = choice.trim().trim_end_matches("ms").trim().parse::<u32>();
        let Ok(raw) = parsed else {
            return Outcome::persisted();
        };
        let value = deadsync_profile::clamp_long_error_bar_threshold_ms(raw);
        state.player_options[player_idx].long_error_bar_threshold_ms = value;
        Outcome::persisted()
    },
};

const LONG_ERROR_BAR_MIN_SAMPLES: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let Ok(raw) = choice.trim().parse::<u32>() else {
            return Outcome::persisted();
        };
        let value = deadsync_profile::clamp_long_error_bar_min_samples(raw);
        state.player_options[player_idx].long_error_bar_min_samples = value;
        Outcome::persisted()
    },
};

fn chosen_tilt_threshold_ms(
    state: &mut State,
    player_idx: usize,
    row_id: RowId,
    delta: isize,
    wrap: NavWrap,
) -> Option<u32> {
    let new_index = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)?;
    let choice = state
        .pane()
        .row_map
        .get(row_id)
        .and_then(|r| r.choices.get(new_index))?;
    parse_tilt_threshold_ms(choice)
}

fn set_tilt_threshold_row(state: &mut State, player_idx: usize, row_id: RowId, ms: u32) {
    let needle = fmt_tilt_threshold_ms(ms);
    if let Some(row) = state.pane_mut().row_map.get_mut(row_id)
        && let Some(idx) = row.choices.iter().position(|choice| choice == &needle)
    {
        row.selected_choice_index[player_idx] = idx;
    }
}

const JUDGMENT_TILT_MIN_THRESHOLD: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(min_ms) = chosen_tilt_threshold_ms(state, player_idx, row_id, delta, wrap) else {
            return Outcome::NONE;
        };
        let max_ms = {
            let profile = &mut state.player_options[player_idx];
            let min_ms = deadsync_profile::clamp_tilt_threshold_ms(min_ms);
            let max_ms = deadsync_profile::clamp_tilt_threshold_ms(profile.tilt_max_threshold_ms)
                .max(min_ms);
            profile.tilt_min_threshold_ms = min_ms;
            profile.tilt_max_threshold_ms = max_ms;
            max_ms
        };
        set_tilt_threshold_row(state, player_idx, RowId::JudgmentTiltMaxThreshold, max_ms);
        Outcome::persisted()
    },
};

const JUDGMENT_TILT_MAX_THRESHOLD: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(max_ms) = chosen_tilt_threshold_ms(state, player_idx, row_id, delta, wrap) else {
            return Outcome::NONE;
        };
        let min_ms = {
            let profile = &mut state.player_options[player_idx];
            let max_ms = deadsync_profile::clamp_tilt_threshold_ms(max_ms);
            let min_ms = deadsync_profile::clamp_tilt_threshold_ms(profile.tilt_min_threshold_ms)
                .min(max_ms);
            profile.tilt_min_threshold_ms = min_ms;
            profile.tilt_max_threshold_ms = max_ms;
            min_ms
        };
        set_tilt_threshold_row(state, player_idx, RowId::JudgmentTiltMinThreshold, min_ms);
        Outcome::persisted()
    },
};

const MEASURE_COUNTER_LOOKAHEAD: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let lookahead = (new_index as u8).min(4);
        state.player_options[player_idx].measure_counter_lookahead = lookahead;
        Outcome::persisted()
    },
};

const CUSTOM_BLUE_FANTASTIC_WINDOW_MS: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let ms = deadsync_profile::clamp_custom_fantastic_window_ms(raw);
        state.player_options[player_idx].custom_fantastic_window_ms = ms;
        Outcome::persisted()
    },
};

const CROSSOVER_CUE_DURATION: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let Ok(raw) = choice.trim_end_matches("ms").parse::<u16>() else {
            return Outcome::persisted();
        };
        let ms = deadsync_profile::clamp_crossover_cue_duration_ms(raw);
        state.player_options[player_idx].crossover_cue_duration_ms = ms;
        Outcome::persisted()
    },
};

const CROSSOVER_CUE_QUANTIZATION: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let Ok(raw) = choice.parse::<u8>() else {
            return Outcome::persisted();
        };
        let q = deadsync_profile::clamp_crossover_cue_quantization(raw);
        state.player_options[player_idx].crossover_cue_quantization = q;
        Outcome::persisted()
    },
};

#[inline(always)]
fn step_stats_extra_label_key(setting: StepStatsExtra) -> &'static str {
    match setting {
        StepStatsExtra::None => "StepStatsExtraNone",
        StepStatsExtra::ErrorStats => "StepStatsExtraErrorStats",
        StepStatsExtra::AmongUs => "StepStatsExtraAmongUs",
        StepStatsExtra::Bocchi => "StepStatsExtraBocchi",
        StepStatsExtra::BrodyQuest => "StepStatsExtraBrodyQuest",
        StepStatsExtra::CatJAM => "StepStatsExtraCatJAM",
        StepStatsExtra::CrabPls => "StepStatsExtraCrabPls",
        StepStatsExtra::DancingDuck => "StepStatsExtraDancingDuck",
        StepStatsExtra::DonChan => "StepStatsExtraDonChan",
        StepStatsExtra::NyanCat => "StepStatsExtraNyanCat",
        StepStatsExtra::Randomizer => "StepStatsExtraRandomizer",
        StepStatsExtra::RinCat => "StepStatsExtraRinCat",
        StepStatsExtra::Snoop => "StepStatsExtraSnoop",
        StepStatsExtra::Sonic => "StepStatsExtraSonic",
    }
}

pub(super) fn build_advanced_rows(return_screen: Screen, scorebox_available: bool) -> RowMap {
    let pack_info_label = if return_screen == Screen::SelectCourse {
        tr("PlayerOptions", "StepStatisticsCourseBanner")
    } else {
        tr("PlayerOptions", "StepStatisticsPackInfo")
    };
    let mut gameplay_extras_choices = vec![
        tr("PlayerOptions", "GameplayExtrasFlashColumnForMiss").to_string(),
        tr("PlayerOptions", "GameplayExtrasDensityGraphAtTop").to_string(),
        tr("PlayerOptions", "GameplayExtrasColumnCues").to_string(),
        tr("PlayerOptions", "GameplayExtrasMeasureCues").to_string(),
        tr("PlayerOptions", "GameplayExtrasLiveTimingStats").to_string(),
        tr("PlayerOptions", "GameplayExtrasColumnCountdown").to_string(),
    ];
    if scorebox_available {
        gameplay_extras_choices
            .push(tr("PlayerOptions", "GameplayExtrasDisplayScorebox").to_string());
    }
    let column_flash_choices = vec![
        tr("PlayerOptions", "ColumnFlashBlueFantastic").to_string(),
        tr("PlayerOptions", "ColumnFlashWhiteFantastic").to_string(),
        tr("PlayerOptions", "ColumnFlashExcellent").to_string(),
        tr("PlayerOptions", "ColumnFlashGreat").to_string(),
        tr("PlayerOptions", "ColumnFlashDecent").to_string(),
        tr("PlayerOptions", "ColumnFlashWayOff").to_string(),
        tr("PlayerOptions", "ColumnFlashMiss").to_string(),
    ];
    let live_timing_stats_choices = vec![
        tr("PlayerOptions", "LiveTimingStatsMean").to_string(),
        tr("PlayerOptions", "LiveTimingStatsMeanAbs").to_string(),
        tr("PlayerOptions", "LiveTimingStatsMax").to_string(),
    ];

    let mut b = RowBuilder::new();
    b.push(Row::cycle(
        RowId::Turn,
        lookup_key("PlayerOptions", "Turn"),
        lookup_key("PlayerOptionsHelp", "TurnHelp"),
        CycleBinding::Index(TURN),
        vec![
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
    ));
    b.push(Row::bitmask(
        RowId::Scroll,
        lookup_key("PlayerOptions", "Scroll"),
        lookup_key("PlayerOptionsHelp", "ScrollHelp"),
        SCROLL,
        vec![
            tr("PlayerOptions", "ScrollReverse").to_string(),
            tr("PlayerOptions", "ScrollSplit").to_string(),
            tr("PlayerOptions", "ScrollAlternate").to_string(),
            tr("PlayerOptions", "ScrollCross").to_string(),
            tr("PlayerOptions", "ScrollCentered").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::Hide,
        lookup_key("PlayerOptions", "Hide"),
        lookup_key("PlayerOptionsHelp", "HideHelp"),
        HIDE,
        vec![
            tr("PlayerOptions", "HideTargets").to_string(),
            tr("PlayerOptions", "HideBackground").to_string(),
            tr("PlayerOptions", "HideCombo").to_string(),
            tr("PlayerOptions", "HideLife").to_string(),
            tr("PlayerOptions", "HideScore").to_string(),
            tr("PlayerOptions", "HideDanger").to_string(),
            tr("PlayerOptions", "HideComboExplosions").to_string(),
            tr("PlayerOptions", "HideUsername").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::LifeMeterType,
        lookup_key("PlayerOptions", "LifeMeterType"),
        lookup_key("PlayerOptionsHelp", "LifeMeterTypeHelp"),
        CycleBinding::Index(LIFE_METER_TYPE),
        vec![
            tr("PlayerOptions", "LifeMeterTypeStandard").to_string(),
            tr("PlayerOptions", "LifeMeterTypeSurround").to_string(),
            tr("PlayerOptions", "LifeMeterTypeVertical").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::LifeBarOptions,
        lookup_key("PlayerOptions", "LifeBarOptions"),
        lookup_key("PlayerOptionsHelp", "LifeBarOptionsHelp"),
        LIFE_BAR_OPTIONS,
        vec![
            tr("PlayerOptions", "LifeBarOptionsRainbowMax").to_string(),
            tr("PlayerOptions", "LifeBarOptionsResponsiveColors").to_string(),
            tr("PlayerOptions", "LifeBarOptionsShowLifePercentage").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::DataVisualizations,
        lookup_key("PlayerOptions", "StepStatistics"),
        lookup_key("PlayerOptionsHelp", "StepStatisticsHelp"),
        STEP_STATISTICS,
        vec![
            tr("PlayerOptions", "StepStatisticsDensity").to_string(),
            tr("PlayerOptions", "StepStatisticsBanner").to_string(),
            tr("PlayerOptions", "StepStatisticsJudgements").to_string(),
            tr("PlayerOptions", "StepStatisticsDuration").to_string(),
            pack_info_label.to_string(),
            tr("PlayerOptions", "StepStatisticsStepCounts").to_string(),
            tr("PlayerOptions", "StepStatisticsPeakNps").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::StepStatsExtra,
        lookup_key("PlayerOptions", "StepStatsExtra"),
        lookup_key("PlayerOptionsHelp", "StepStatsExtraHelp"),
        CycleBinding::Index(STEP_STATS_EXTRA),
        STEP_STATS_EXTRA_VARIANTS
            .iter()
            .map(|&setting| tr("PlayerOptions", step_stats_extra_label_key(setting)).to_string())
            .collect(),
    ));
    b.push(Row::cycle(
        RowId::DensityGraphBackground,
        lookup_key("PlayerOptions", "DensityGraphBackground"),
        lookup_key("PlayerOptionsHelp", "DensityGraphBackgroundHelp"),
        CycleBinding::Bool(DENSITY_GRAPH_BACKGROUND),
        vec![
            tr("PlayerOptions", "DensityGraphBackgroundSolid").to_string(),
            tr("PlayerOptions", "DensityGraphBackgroundTransparent").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::ScoreDisplay,
        lookup_key("PlayerOptions", "ScoreDisplay"),
        lookup_key("PlayerOptionsHelp", "ScoreDisplayHelp"),
        CycleBinding::Index(SCORE_DISPLAY_MODE),
        vec![
            tr("PlayerOptions", "ScoreDisplayNormal").to_string(),
            tr("PlayerOptions", "ScoreDisplayPredictive").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::ScorePosition,
        lookup_key("PlayerOptions", "ScorePosition"),
        lookup_key("PlayerOptionsHelp", "ScorePositionHelp"),
        CycleBinding::Index(SCORE_POSITION),
        vec![
            tr("PlayerOptions", "ScorePositionNormal").to_string(),
            tr("PlayerOptions", "ScorePositionStepStatistics").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::ActionOnMissedTarget,
        lookup_key("PlayerOptions", "TargetScoreMissPolicy"),
        lookup_key("PlayerOptionsHelp", "TargetScoreMissPolicyHelp"),
        ACTION_ON_MISSED_TARGET,
        vec![
            tr("PlayerOptions", "TargetScoreMissPolicyNothing").to_string(),
            tr("PlayerOptions", "TargetScoreMissPolicyFail").to_string(),
            tr("PlayerOptions", "TargetScoreMissPolicyRestartSong").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::MiniIndicator,
        lookup_key("PlayerOptions", "MiniIndicator"),
        lookup_key("PlayerOptionsHelp", "MiniIndicatorHelp"),
        MINI_INDICATOR,
        vec![
            tr("PlayerOptions", "MiniIndicatorNone").to_string(),
            tr("PlayerOptions", "MiniIndicatorSubtractiveScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPredictiveScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPaceScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorRivalScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPacemaker").to_string(),
            tr("PlayerOptions", "MiniIndicatorStreamProg").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::IndicatorScoreType,
        lookup_key("PlayerOptions", "IndicatorScoreType"),
        lookup_key("PlayerOptionsHelp", "IndicatorScoreTypeHelp"),
        CycleBinding::Index(INDICATOR_SCORE_TYPE),
        vec![
            tr("PlayerOptions", "IndicatorScoreTypeITG").to_string(),
            tr("PlayerOptions", "IndicatorScoreTypeEX").to_string(),
            tr("PlayerOptions", "IndicatorScoreTypeHEX").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::MiniIndicatorSubtractiveDisplay,
        lookup_key("PlayerOptions", "MiniIndicatorSubtractiveDisplay"),
        lookup_key("PlayerOptionsHelp", "MiniIndicatorSubtractiveDisplayHelp"),
        CycleBinding::Index(MINI_INDICATOR_SUBTRACTIVE_DISPLAY),
        vec![
            tr("PlayerOptions", "MiniIndicatorSubtractiveDisplayPercent").to_string(),
            tr("PlayerOptions", "MiniIndicatorSubtractiveDisplayPoints").to_string(),
        ],
    ));
    b.push(
        Row::cycle(
            RowId::TargetScore,
            lookup_key("PlayerOptions", "TargetScore"),
            lookup_key("PlayerOptionsHelp", "TargetScoreHelp"),
            CycleBinding::Index(TARGET_SCORE),
            vec![
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
        )
        .with_initial_choice_index(10), // S by default
    );
    b.push(Row::cycle(
        RowId::MiniIndicatorSize,
        lookup_key("PlayerOptions", "MiniIndicatorSize"),
        lookup_key("PlayerOptionsHelp", "MiniIndicatorSizeHelp"),
        CycleBinding::Index(MINI_INDICATOR_SIZE),
        vec![
            tr("PlayerOptions", "MiniIndicatorSizeDefault").to_string(),
            tr("PlayerOptions", "MiniIndicatorSizeLarge").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::MiniIndicatorColor,
        lookup_key("PlayerOptions", "MiniIndicatorColor"),
        lookup_key("PlayerOptionsHelp", "MiniIndicatorColorHelp"),
        CycleBinding::Index(MINI_INDICATOR_COLOR),
        vec![
            tr("PlayerOptions", "MiniIndicatorColorDefault").to_string(),
            tr("PlayerOptions", "MiniIndicatorColorDetailed").to_string(),
            tr("PlayerOptions", "MiniIndicatorColorCombo").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::MiniIndicatorPosition,
        lookup_key("PlayerOptions", "MiniIndicatorPosition"),
        lookup_key("PlayerOptionsHelp", "MiniIndicatorPositionHelp"),
        CycleBinding::Index(MINI_INDICATOR_POSITION),
        vec![
            tr("PlayerOptions", "MiniIndicatorPositionDefault").to_string(),
            tr("PlayerOptions", "MiniIndicatorPositionUnderUpArrow").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::GameplayExtras,
        lookup_key("PlayerOptions", "GameplayExtras"),
        lookup_key("PlayerOptionsHelp", "GameplayExtrasHelp"),
        GAMEPLAY_EXTRAS,
        gameplay_extras_choices,
    ));
    b.push(Row::bitmask(
        RowId::ColumnFlashJudgments,
        lookup_key("PlayerOptions", "ColumnFlashJudgments"),
        lookup_key("PlayerOptionsHelp", "ColumnFlashJudgmentsHelp"),
        COLUMN_FLASH_JUDGMENTS,
        column_flash_choices,
    ));
    b.push(Row::cycle(
        RowId::ColumnFlashBrightness,
        lookup_key("PlayerOptions", "ColumnFlashBrightness"),
        lookup_key("PlayerOptionsHelp", "ColumnFlashBrightnessHelp"),
        CycleBinding::Index(COLUMN_FLASH_BRIGHTNESS),
        vec![
            tr("PlayerOptions", "ColumnFlashBrightnessNormal").to_string(),
            tr("PlayerOptions", "ColumnFlashBrightnessDimmed").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::ColumnFlashSize,
        lookup_key("PlayerOptions", "ColumnFlashSize"),
        lookup_key("PlayerOptionsHelp", "ColumnFlashSizeHelp"),
        CycleBinding::Index(COLUMN_FLASH_SIZE),
        vec![
            tr("PlayerOptions", "ColumnFlashSizeDefault").to_string(),
            tr("PlayerOptions", "ColumnFlashSizeCompact").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::LiveTimingStats,
        lookup_key("PlayerOptions", "LiveTimingStats"),
        lookup_key("PlayerOptionsHelp", "LiveTimingStatsHelp"),
        LIVE_TIMING_STATS,
        live_timing_stats_choices,
    ));
    b.push(Row::cycle(
        RowId::CrossoverCues,
        lookup_key("PlayerOptions", "CrossoverCues"),
        lookup_key("PlayerOptionsHelp", "CrossoverCuesHelp"),
        CycleBinding::Bool(CROSSOVER_CUES),
        vec![
            tr("PlayerOptions", "CrossoverCuesOff").to_string(),
            tr("PlayerOptions", "CrossoverCuesOn").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::CrossoverCueDuration,
        lookup_key("PlayerOptions", "CrossoverCueDuration"),
        lookup_key("PlayerOptionsHelp", "CrossoverCueDurationHelp"),
        CROSSOVER_CUE_DURATION,
        crossover_cue_duration_choices(),
    ));
    b.push(Row::custom(
        RowId::CrossoverCueQuantization,
        lookup_key("PlayerOptions", "CrossoverCueQuantization"),
        lookup_key("PlayerOptionsHelp", "CrossoverCueQuantizationHelp"),
        CROSSOVER_CUE_QUANTIZATION,
        crossover_cue_quantization_choices(),
    ));
    b.push(Row::cycle(
        RowId::CrossoverCueBrackets,
        lookup_key("PlayerOptions", "CrossoverCueBrackets"),
        lookup_key("PlayerOptionsHelp", "CrossoverCueBracketsHelp"),
        CycleBinding::Bool(CROSSOVER_CUE_BRACKETS),
        vec![
            tr("PlayerOptions", "CrossoverCueBracketsOff").to_string(),
            tr("PlayerOptions", "CrossoverCueBracketsOn").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::ComboColors,
        lookup_key("PlayerOptions", "ComboColors"),
        lookup_key("PlayerOptionsHelp", "ComboColorsHelp"),
        CycleBinding::Index(COMBO_COLORS),
        vec![
            tr("PlayerOptions", "ComboColorsGlow").to_string(),
            tr("PlayerOptions", "ComboColorsSolid").to_string(),
            tr("PlayerOptions", "ComboColorsRainbow").to_string(),
            tr("PlayerOptions", "ComboColorsRainbowScroll").to_string(),
            tr("PlayerOptions", "ComboColorsNone").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::ComboColorMode,
        lookup_key("PlayerOptions", "ComboColorMode"),
        lookup_key("PlayerOptionsHelp", "ComboColorModeHelp"),
        CycleBinding::Index(COMBO_COLOR_MODE),
        vec![
            tr("PlayerOptions", "ComboColorModeFullCombo").to_string(),
            tr("PlayerOptions", "ComboColorModeCurrentCombo").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::CarryCombo,
        lookup_key("PlayerOptions", "CarryCombo"),
        lookup_key("PlayerOptionsHelp", "CarryComboHelp"),
        CycleBinding::Bool(CARRY_COMBO),
        vec![
            tr("PlayerOptions", "CarryComboNo").to_string(),
            tr("PlayerOptions", "CarryComboYes").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::JudgmentTilt,
        lookup_key("PlayerOptions", "JudgmentTilt"),
        lookup_key("PlayerOptionsHelp", "JudgmentTiltHelp"),
        CycleBinding::Bool(JUDGMENT_TILT),
        vec![
            tr("PlayerOptions", "JudgmentTiltNo").to_string(),
            tr("PlayerOptions", "JudgmentTiltYes").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::JudgmentTiltIntensity,
        lookup_key("PlayerOptions", "JudgmentTiltIntensity"),
        lookup_key("PlayerOptionsHelp", "JudgmentTiltIntensityHelp"),
        JUDGMENT_TILT_INTENSITY,
        tilt_intensity_choices(),
    ));
    b.push(Row::custom(
        RowId::JudgmentTiltMinThreshold,
        lookup_key("PlayerOptions", "JudgmentTiltMinThreshold"),
        lookup_key("PlayerOptionsHelp", "JudgmentTiltMinThresholdHelp"),
        JUDGMENT_TILT_MIN_THRESHOLD,
        tilt_threshold_choices(),
    ));
    b.push(Row::custom(
        RowId::JudgmentTiltMaxThreshold,
        lookup_key("PlayerOptions", "JudgmentTiltMaxThreshold"),
        lookup_key("PlayerOptionsHelp", "JudgmentTiltMaxThresholdHelp"),
        JUDGMENT_TILT_MAX_THRESHOLD,
        tilt_threshold_choices(),
    ));
    b.push(Row::cycle(
        RowId::JudgmentBehindArrows,
        lookup_key("PlayerOptions", "JudgmentBehindArrows"),
        lookup_key("PlayerOptionsHelp", "JudgmentBehindArrowsHelp"),
        CycleBinding::Bool(JUDGMENT_BEHIND_ARROWS),
        vec![
            tr("PlayerOptions", "JudgmentBehindArrowsOff").to_string(),
            tr("PlayerOptions", "JudgmentBehindArrowsOn").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::OffsetIndicator,
        lookup_key("PlayerOptions", "OffsetIndicator"),
        lookup_key("PlayerOptionsHelp", "OffsetIndicatorHelp"),
        CycleBinding::Bool(OFFSET_INDICATOR),
        vec![
            tr("PlayerOptions", "OffsetIndicatorOff").to_string(),
            tr("PlayerOptions", "OffsetIndicatorOn").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::ErrorBar,
        lookup_key("PlayerOptions", "ErrorBar"),
        lookup_key("PlayerOptionsHelp", "ErrorBarHelp"),
        ERROR_BAR,
        vec![
            tr("PlayerOptions", "ErrorBarColorful").to_string(),
            tr("PlayerOptions", "ErrorBarMonochrome").to_string(),
            tr("PlayerOptions", "ErrorBarText").to_string(),
            tr("PlayerOptions", "ErrorBarHighlight").to_string(),
            tr("PlayerOptions", "ErrorBarAverage").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::TextErrorBarMode,
        lookup_key("PlayerOptions", "TextErrorBarMode"),
        lookup_key("PlayerOptionsHelp", "TextErrorBarModeHelp"),
        CycleBinding::Bool(TEXT_ERROR_BAR_MODE),
        vec![
            tr("PlayerOptions", "TextErrorBarModeWindow").to_string(),
            tr("PlayerOptions", "TextErrorBarModeScalable").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::TextErrorBarThreshold,
        lookup_key("PlayerOptions", "TextErrorBarThreshold"),
        lookup_key("PlayerOptionsHelp", "TextErrorBarThresholdHelp"),
        TEXT_ERROR_BAR_THRESHOLD,
        text_error_bar_threshold_choices(),
    ));
    b.push(Row::cycle(
        RowId::ShortAverageErrorBar,
        lookup_key("PlayerOptions", "ShortAverageErrorBar"),
        lookup_key("PlayerOptionsHelp", "ShortAverageErrorBarHelp"),
        CycleBinding::Bool(SHORT_AVERAGE_ERROR_BAR),
        vec![
            tr("PlayerOptions", "ShortAverageErrorBarOff").to_string(),
            tr("PlayerOptions", "ShortAverageErrorBarOn").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::CenterTick,
        lookup_key("PlayerOptions", "CenterTick"),
        lookup_key("PlayerOptionsHelp", "CenterTickHelp"),
        CycleBinding::Bool(CENTER_TICK),
        vec![
            tr("PlayerOptions", "CenterTickOff").to_string(),
            tr("PlayerOptions", "CenterTickOn").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::AverageErrorBarIntensity,
        lookup_key("PlayerOptions", "AverageErrorBarIntensity"),
        lookup_key("PlayerOptionsHelp", "AverageErrorBarIntensityHelp"),
        AVERAGE_ERROR_BAR_INTENSITY,
        average_error_bar_intensity_choices(),
    ));
    b.push(Row::custom(
        RowId::AverageErrorBarInterval,
        lookup_key("PlayerOptions", "AverageErrorBarInterval"),
        lookup_key("PlayerOptionsHelp", "AverageErrorBarIntervalHelp"),
        AVERAGE_ERROR_BAR_INTERVAL,
        average_error_bar_interval_choices(),
    ));
    b.push(Row::cycle(
        RowId::ErrorBarTrim,
        lookup_key("PlayerOptions", "ErrorBarTrim"),
        lookup_key("PlayerOptionsHelp", "ErrorBarTrimHelp"),
        CycleBinding::Index(ERROR_BAR_TRIM),
        vec![
            tr("PlayerOptions", "ErrorBarTrimOff").to_string(),
            tr("PlayerOptions", "ErrorBarTrimFantastic").to_string(),
            tr("PlayerOptions", "ErrorBarTrimExcellent").to_string(),
            tr("PlayerOptions", "ErrorBarTrimGreat").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::ErrorBarOptions,
        lookup_key("PlayerOptions", "ErrorBarOptions"),
        lookup_key("PlayerOptionsHelp", "ErrorBarOptionsHelp"),
        ERROR_BAR_OPTIONS,
        vec![
            tr("PlayerOptions", "ErrorBarOptionsMoveUp").to_string(),
            tr("PlayerOptions", "ErrorBarOptionsMultiTick").to_string(),
        ],
    ));
    b.push(
        Row::numeric(
            RowId::ErrorBarOffsetX,
            lookup_key("PlayerOptions", "ErrorBarOffsetX"),
            lookup_key("PlayerOptionsHelp", "ErrorBarOffsetXHelp"),
            ERROR_BAR_OFFSET_X,
            hud_offset_choices(),
        )
        .with_initial_choice_index(HUD_OFFSET_ZERO_INDEX),
    );
    b.push(
        Row::numeric(
            RowId::ErrorBarOffsetY,
            lookup_key("PlayerOptions", "ErrorBarOffsetY"),
            lookup_key("PlayerOptionsHelp", "ErrorBarOffsetYHelp"),
            ERROR_BAR_OFFSET_Y,
            hud_offset_choices(),
        )
        .with_initial_choice_index(HUD_OFFSET_ZERO_INDEX),
    );
    b.push(Row::cycle(
        RowId::LongErrorBar,
        lookup_key("PlayerOptions", "LongErrorBar"),
        lookup_key("PlayerOptionsHelp", "LongErrorBarHelp"),
        CycleBinding::Bool(LONG_ERROR_BAR),
        vec![
            tr("PlayerOptions", "LongErrorBarOff").to_string(),
            tr("PlayerOptions", "LongErrorBarOn").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::LongErrorBarIntensity,
        lookup_key("PlayerOptions", "LongErrorBarIntensity"),
        lookup_key("PlayerOptionsHelp", "LongErrorBarIntensityHelp"),
        LONG_ERROR_BAR_INTENSITY,
        long_error_bar_intensity_choices(),
    ));
    b.push(Row::custom(
        RowId::LongErrorBarThreshold,
        lookup_key("PlayerOptions", "LongErrorBarThreshold"),
        lookup_key("PlayerOptionsHelp", "LongErrorBarThresholdHelp"),
        LONG_ERROR_BAR_THRESHOLD,
        long_error_bar_threshold_choices(),
    ));
    b.push(Row::custom(
        RowId::LongErrorBarMinSamples,
        lookup_key("PlayerOptions", "LongErrorBarMinSamples"),
        lookup_key("PlayerOptionsHelp", "LongErrorBarMinSamplesHelp"),
        LONG_ERROR_BAR_MIN_SAMPLES,
        long_error_bar_min_samples_choices(),
    ));
    b.push(Row::cycle(
        RowId::MeasureCounter,
        lookup_key("PlayerOptions", "MeasureCounter"),
        lookup_key("PlayerOptionsHelp", "MeasureCounterHelp"),
        CycleBinding::Index(MEASURE_COUNTER),
        vec![
            tr("PlayerOptions", "MeasureCounterNone").to_string(),
            tr("PlayerOptions", "MeasureCounter8th").to_string(),
            tr("PlayerOptions", "MeasureCounter12th").to_string(),
            tr("PlayerOptions", "MeasureCounter16th").to_string(),
            tr("PlayerOptions", "MeasureCounter24th").to_string(),
            tr("PlayerOptions", "MeasureCounter32nd").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::MeasureCounterLookahead,
        lookup_key("PlayerOptions", "MeasureCounterLookahead"),
        lookup_key("PlayerOptionsHelp", "MeasureCounterLookaheadHelp"),
        MEASURE_COUNTER_LOOKAHEAD,
        vec![
            tr("PlayerOptions", "MeasureCounterLookahead0").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead1").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead2").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead3").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead4").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::MeasureCounterOptions,
        lookup_key("PlayerOptions", "MeasureCounterOptions"),
        lookup_key("PlayerOptionsHelp", "MeasureCounterOptionsHelp"),
        MEASURE_COUNTER_OPTIONS,
        vec![
            tr("PlayerOptions", "MeasureCounterOptionsMoveLeft").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsMoveUp").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsVerticalLookahead").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsBrokenRunTotal").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsRunTimer").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::MeasureLines,
        lookup_key("PlayerOptions", "MeasureLines"),
        lookup_key("PlayerOptionsHelp", "MeasureLinesHelp"),
        CycleBinding::Index(MEASURE_LINES),
        vec![
            tr("PlayerOptions", "MeasureLinesOff").to_string(),
            tr("PlayerOptions", "MeasureLinesMeasure").to_string(),
            tr("PlayerOptions", "MeasureLinesQuarter").to_string(),
            tr("PlayerOptions", "MeasureLinesEighth").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::RescoreEarlyHits,
        lookup_key("PlayerOptions", "RescoreEarlyHits"),
        lookup_key("PlayerOptionsHelp", "RescoreEarlyHitsHelp"),
        CycleBinding::Bool(RESCORE_EARLY_HITS),
        vec![
            tr("PlayerOptions", "RescoreEarlyHitsNo").to_string(),
            tr("PlayerOptions", "RescoreEarlyHitsYes").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::EarlyDecentWayOffOptions,
        lookup_key("PlayerOptions", "EarlyDecentWayOffOptions"),
        lookup_key("PlayerOptionsHelp", "EarlyDecentWayOffOptionsHelp"),
        EARLY_DW_OPTIONS,
        vec![
            tr("PlayerOptions", "EarlyDecentWayOffOptionsHideJudgments").to_string(),
            tr(
                "PlayerOptions",
                "EarlyDecentWayOffOptionsHideNoteFieldFlash",
            )
            .to_string(),
            tr("PlayerOptions", "EarlyDecentWayOffOptionsHideColumnFlash").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::ResultsExtras,
        lookup_key("PlayerOptions", "ResultsExtras"),
        lookup_key("PlayerOptionsHelp", "ResultsExtrasHelp"),
        RESULTS_EXTRAS,
        vec![
            tr("PlayerOptions", "ResultsExtrasTrackEarlyJudgments").to_string(),
            tr("PlayerOptions", "ResultsExtrasScaleScatterplot").to_string(),
            tr("PlayerOptions", "ResultsExtrasDimPostFailScatter").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::ScatterplotMaxWindow,
        lookup_key("PlayerOptions", "ScatterplotMaxWindow"),
        lookup_key("PlayerOptionsHelp", "ScatterplotMaxWindowHelp"),
        CycleBinding::Index(SCATTERPLOT_MAX_WINDOW),
        vec![
            tr("PlayerOptions", "ScatterplotMaxWindowOff").to_string(),
            tr("PlayerOptions", "ScatterplotMaxWindowFantastic").to_string(),
            tr("PlayerOptions", "ScatterplotMaxWindowExcellent").to_string(),
            tr("PlayerOptions", "ScatterplotMaxWindowGreat").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::TimingWindows,
        lookup_key("PlayerOptions", "TimingWindows"),
        lookup_key("PlayerOptionsHelp", "TimingWindowsHelp"),
        CycleBinding::Index(TIMING_WINDOWS),
        vec![
            tr("PlayerOptions", "TimingWindowsNone").to_string(),
            tr("PlayerOptions", "TimingWindowsWayOffs").to_string(),
            tr("PlayerOptions", "TimingWindowsDecentsAndWayOffs").to_string(),
            tr("PlayerOptions", "TimingWindowsFantasticsAndExcellents").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::FAPlusOptions,
        lookup_key("PlayerOptions", "FAPlusOptions"),
        lookup_key("PlayerOptionsHelp", "FAPlusOptionsHelp"),
        FA_PLUS_OPTIONS,
        vec![
            tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusWindow").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayEXScore").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayHEXScore").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusPane").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::FAPlusWindowOptions,
        lookup_key("PlayerOptions", "FAPlusWindowOptions"),
        lookup_key("PlayerOptionsHelp", "FAPlusWindowOptionsHelp"),
        FA_PLUS_WINDOW_OPTIONS,
        vec![
            tr("PlayerOptions", "FAPlusOptions10msBlueWindow").to_string(),
            tr("PlayerOptions", "FAPlusOptions1510msSplit").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::CustomBlueFantasticWindow,
        lookup_key("PlayerOptions", "CustomBlueFantasticWindow"),
        lookup_key("PlayerOptionsHelp", "CustomBlueFantasticWindowHelp"),
        CycleBinding::Bool(CUSTOM_BLUE_FANTASTIC_WINDOW),
        vec![
            tr("PlayerOptions", "CustomBlueFantasticWindowNo").to_string(),
            tr("PlayerOptions", "CustomBlueFantasticWindowYes").to_string(),
        ],
    ));
    b.push(Row::custom(
        RowId::CustomBlueFantasticWindowMs,
        lookup_key("PlayerOptions", "CustomBlueFantasticWindowMs"),
        lookup_key("PlayerOptionsHelp", "CustomBlueFantasticWindowMsHelp"),
        CUSTOM_BLUE_FANTASTIC_WINDOW_MS,
        custom_fantastic_window_choices(),
    ));
    b.push(Row::cycle(
        RowId::SmxFsrDisplay,
        lookup_key("PlayerOptions", "SmxFsrDisplay"),
        lookup_key("PlayerOptionsHelp", "SmxFsrDisplayHelp"),
        CycleBinding::Bool(SMX_FSR_DISPLAY),
        vec![
            tr("Common", "No").to_string(),
            tr("Common", "Yes").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::SmxPadInputDisplay,
        lookup_key("PlayerOptions", "SmxPadInputDisplay"),
        lookup_key("PlayerOptionsHelp", "SmxPadInputDisplayHelp"),
        CycleBinding::Bool(SMX_PAD_INPUT_DISPLAY),
        vec![
            tr("Common", "No").to_string(),
            tr("Common", "Yes").to_string(),
        ],
    ));
    // "What Comes Next" dictates the screen after this one and must always be
    // the last selectable option, just before Exit.
    b.push(
        Row::custom(
            RowId::WhatComesNext,
            lookup_key("PlayerOptions", "WhatComesNext"),
            lookup_key("PlayerOptionsHelp", "WhatComesNextAdvancedHelp"),
            super::WHAT_COMES_NEXT,
            what_comes_next_choices(OptionsPane::Advanced, return_screen),
        )
        .with_mirror_across_players(),
    );
    b.push(Row::exit());
    b.finish()
}

#[cfg(test)]
mod bitmask_binding_init_tests {
    use super::super::super::row::{Row, RowBehavior, RowId, init_bitmask_row_from_binding};
    use super::super::super::state::{FaPlusMask, HideMask, PlayerOptionMasks};
    use super::*;
    use crate::assets::i18n::{LookupKey, lookup_key};
    use deadsync_profile::PlayerOptionsData;

    fn ensure_i18n() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            crate::assets::i18n::init_for_tests();
        });
    }

    fn make_bitmask_row(id: RowId, name: LookupKey, choices: &[&str]) -> Row {
        // Placeholder Generic binding — these tests call
        // `init_bitmask_row_from_binding` with a *different* (production)
        // binding, so the row's own behavior is never exercised.
        let stub = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |_| 0,
                get_active: |_| 0,
                set_active: |_, _| {},
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, _, _| {},
                bit_mapping: BitMapping::Sequential { width: 0 },
                sync_visibility: false,
            },
        };
        Row {
            id,
            behavior: RowBehavior::Bitmask(stub),
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
        let mut profile = PlayerOptionsData::default();
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
        let mut profile = PlayerOptionsData::default();
        profile.show_fa_plus_window = false;
        profile.show_ex_score = true;

        let mut row = make_bitmask_row(
            RowId::FAPlusOptions,
            lookup_key("PlayerOptions", "FAPlusOptions"),
            &["Window", "EX", "HardEX", "Pane"],
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

    #[test]
    fn fa_plus_window_binding_reads_shifted_child_bits() {
        ensure_i18n();
        let mut profile = PlayerOptionsData::default();
        profile.split_15_10ms = true;

        let mut row = make_bitmask_row(
            RowId::FAPlusWindowOptions,
            lookup_key("PlayerOptions", "FAPlusWindowOptions"),
            &["Blue10", "Split"],
        );
        let mut masks = PlayerOptionMasks::default();
        let applied = init_bitmask_row_from_binding(
            &mut row,
            &FA_PLUS_WINDOW_OPTIONS,
            &profile,
            &mut masks,
            0,
        );
        assert!(applied, "FA_PLUS_WINDOW_OPTIONS binding has init contract");
        assert_eq!(
            masks.fa_plus,
            FaPlusMask::SPLIT_15_10MS,
            "child row init preserves the shared FA+ mask bits",
        );
        assert_eq!(
            row.selected_choice_index[0], 1,
            "child row cursor reads shifted bits so Split lands on choice index 1",
        );
    }

    /// Order assertion: Scroll choice index N maps to ScrollMask bit (1 << N)
    /// maps to ScrollOption variant N. The SCROLL binding's from_profile
    /// closure relies on this 1:1 ordering; if any of the three orderings
    /// drifts, this test must fail before reaching production.
    #[test]
    fn scroll_choice_order_matches_scroll_option_bits() {
        use deadsync_profile::ScrollOption;
        let cases = [
            (0u8, ScrollOption::Reverse),
            (1, ScrollOption::Split),
            (2, ScrollOption::Alternate),
            (3, ScrollOption::Cross),
            (4, ScrollOption::Centered),
        ];
        for (idx, opt) in cases {
            let mut profile = PlayerOptionsData::default();
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
