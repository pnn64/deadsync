use super::*;

#[cfg(test)]
pub(super) mod tests {
    use super::super::input::what_comes_next_pane;
    use super::super::panes;
    use super::{
        BitMapping, BitmaskBinding, BitmaskInit, BitmaskWriteback, ChoiceBinding, CursorInit,
        CycleInit, ErrorBarMask, FaPlusMask, GameplayExtrasMask, GameplayExtrasMoreMask,
        HUD_OFFSET_MAX, HUD_OFFSET_MIN, HUD_OFFSET_ZERO_INDEX, HideMask, NAV_INITIAL_HOLD_DELAY,
        NavDirection, NumericBinding, NumericInit, OptionsPane, P1, P2, PlayerOptionMasks, Row,
        RowBehavior, RowId, RowMap, ScrollMask, SpeedMod, SpeedModType, compute_row_window,
        count_visible_rows, effective_scroll_speed_with_alt, handle_arcade_start_event,
        handle_nav_event, handle_start_event, hud_offset_choices, init_cycle_row_from_binding,
        init_numeric_row_from_binding, is_row_visible, judgment_tilt_options_visible,
        on_start_press, player_option_column_x, prepend_pending_audio, queue_audio, queue_sfx,
        repeat_held_arcade_start, row_f_pos_for_index, row_visibility, session_active_players,
        sync_profile_scroll_speed, sync_speed_mod_type_row, update,
    };
    use crate::assets::AssetManager;
    use crate::assets::i18n::{LookupKey, lookup_key};
    use crate::screens::{Screen, ThemeEffect};
    use deadlib_present::font::{Font, Glyph};
    use deadsync_chart::{ChartData, SongData};
    use deadsync_profile::Profile;
    use deadsync_profile::compat as profile;
    use deadsync_profile::{
        BackgroundFilter, ComboFont, NoCmodAlternative, Perspective, PlayStyle, PlayerSide,
        ScrollOption, StepStatisticsMask,
    };
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_theme::AudioRequest;
    use deadsync_theme::views::NoteskinCatalogView;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    fn ensure_i18n() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            crate::assets::i18n::init("en");
        });
    }

    fn test_noteskin_catalog() -> NoteskinCatalogView {
        NoteskinCatalogView {
            names: vec![deadsync_profile::NoteSkin::DEFAULT_NAME.to_owned()],
        }
    }

    fn test_row(
        id: RowId,
        name: LookupKey,
        choices: &[&str],
        selected_choice_index: [usize; 2],
    ) -> Row {
        Row {
            id,
            behavior: super::RowBehavior::Exit,
            name,
            choices: choices.iter().map(ToString::to_string).collect(),
            selected_choice_index,
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        }
    }

    fn test_bitmask_row(
        id: RowId,
        name: LookupKey,
        choices: &[&str],
        binding: BitmaskBinding,
    ) -> Row {
        Row {
            id,
            behavior: RowBehavior::Bitmask(binding),
            name,
            choices: choices.iter().map(ToString::to_string).collect(),
            selected_choice_index: [0, 0],
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        }
    }

    fn test_row_map(rows: Vec<Row>) -> RowMap {
        let mut map = RowMap::new();
        for row in rows {
            map.display_order.push(row.id);
            map.insert(row);
        }
        map
    }

    fn test_song() -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from("tests/player-options/test.ssc"),
            title: "Test Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: "Test Artist".to_string(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: "120".to_string(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120".to_string(),
            music_length_seconds: 120.0,
            first_second: 0.0,
            total_length_seconds: 120,
            precise_last_second_seconds: 120.0,
            charts: vec![test_chart()],
        })
    }

    fn test_chart() -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Hard".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: "player-options-test".to_string(),
            stats: Default::default(),
            tech_counts: Default::default(),
            mines_nonfake: 0,
            stamina_counts: Default::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: false,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn register_test_fonts(asset_manager: &mut AssetManager) {
        for name in ["miso", "wendy", "wendy small", "game chars"] {
            asset_manager.register_font(name, test_font());
        }
    }

    fn test_font() -> Font {
        let texture_key = Arc::<str>::from("test/font.png");
        let glyph = Glyph {
            texture_key,
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 8.0, 16.0],
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            size: [8.0, 16.0],
            offset: [0.0, 0.0],
            advance: 8.0,
            advance_i32: 8,
        };
        let mut glyph_map = HashMap::new();
        for ch in 32u8..=126 {
            glyph_map.insert(char::from(ch), glyph.clone());
        }
        let mut ascii_glyphs = Box::new(std::array::from_fn(|_| None));
        for ch in 32u8..=126 {
            ascii_glyphs[ch as usize] = Some(glyph.clone());
        }
        Font {
            glyph_map,
            ascii_glyphs,
            default_glyph: Some(glyph),
            line_spacing: 20,
            height: 16,
            fallback_font_name: None,
            cache_tag: 0,
            chain_key: 0,
            default_stroke_color: [0.0, 0.0, 0.0, 1.0],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        }
    }

    fn hidden_child_f_pos(row_map: &RowMap) -> (f32, bool) {
        let active = [true, false];
        let masks = [PlayerOptionMasks::default(), PlayerOptionMasks::default()];
        let visibility = row_visibility(row_map, active, masks, false);
        assert!(is_row_visible(row_map, 0, visibility));
        assert!(!is_row_visible(row_map, 1, visibility));

        let visible_rows = count_visible_rows(row_map, visibility);
        let window = compute_row_window(visible_rows, [0, 0], active);
        let mut visible_idx = 0;
        let (parent_f_pos, parent_hidden) =
            row_f_pos_for_index(row_map, 0, visibility, &mut visible_idx, window, 0.0, 0.0);
        assert!(!parent_hidden);
        let (child_f_pos, child_hidden) =
            row_f_pos_for_index(row_map, 1, visibility, &mut visible_idx, window, 0.0, 0.0);
        assert!(
            (child_f_pos - parent_f_pos).abs() < 0.001,
            "hidden child should collapse into its parent row"
        );
        (child_f_pos, child_hidden)
    }

    /// Stub writeback for synthetic test bindings whose tests only exercise
    /// the init contract (`apply_profile_defaults`). The toggle path is
    /// never invoked, so `project`/`persist` semantics are irrelevant; the
    /// mapping still needs to cover the test rows so FirstActiveBit cursor
    /// init can resolve active choices.
    const TEST_WRITEBACK: BitmaskWriteback = BitmaskWriteback {
        project: |_, _, _| {},
        persist_for_side: |_, _| {},
        bit_mapping: BitMapping::Sequential { width: 32 },
        sync_visibility: false,
    };

    #[test]
    fn sync_profile_scroll_speed_matches_speed_mod() {
        let mut profile = Profile::default();

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: SpeedModType::X,
                value: 1.5,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::XMod(1.5));

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: SpeedModType::M,
                value: 750.0,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::MMod(750.0));

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: SpeedModType::C,
                value: 600.0,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::CMod(600.0));
    }

    #[test]
    fn no_cmod_alternative_substitutes_only_cmod_on_no_cmod_charts() {
        let cmod = SpeedMod {
            mod_type: SpeedModType::C,
            value: 600.0,
        };
        // reference_bpm 150, rate 1.0 → C600 is visually X4.0 / M600.
        let (reference_bpm, rate) = (150.0, 1.0);

        // No substitution when the chart is not tagged no-cmod.
        assert_eq!(
            effective_scroll_speed_with_alt(
                &cmod,
                NoCmodAlternative::XMod,
                false,
                reference_bpm,
                rate
            ),
            ScrollSpeedSetting::CMod(600.0)
        );
        // No substitution when the alternative is None.
        assert_eq!(
            effective_scroll_speed_with_alt(
                &cmod,
                NoCmodAlternative::None,
                true,
                reference_bpm,
                rate
            ),
            ScrollSpeedSetting::CMod(600.0)
        );
        // No-cmod chart + CMod + XMod alternative → equivalent XMod.
        assert_eq!(
            effective_scroll_speed_with_alt(
                &cmod,
                NoCmodAlternative::XMod,
                true,
                reference_bpm,
                rate
            ),
            ScrollSpeedSetting::XMod(4.0)
        );
        // No-cmod chart + CMod + MMod alternative → equivalent MMod.
        assert_eq!(
            effective_scroll_speed_with_alt(
                &cmod,
                NoCmodAlternative::MMod,
                true,
                reference_bpm,
                rate
            ),
            ScrollSpeedSetting::MMod(600.0)
        );
        // A player already off CMod is never altered, even on a no-cmod chart.
        let xmod = SpeedMod {
            mod_type: SpeedModType::X,
            value: 2.0,
        };
        assert_eq!(
            effective_scroll_speed_with_alt(
                &xmod,
                NoCmodAlternative::MMod,
                true,
                reference_bpm,
                rate
            ),
            ScrollSpeedSetting::XMod(2.0)
        );
        let mmod = SpeedMod {
            mod_type: SpeedModType::M,
            value: 600.0,
        };
        assert_eq!(
            effective_scroll_speed_with_alt(
                &mmod,
                NoCmodAlternative::XMod,
                true,
                reference_bpm,
                rate
            ),
            ScrollSpeedSetting::MMod(600.0)
        );
    }

    #[test]
    fn sync_speed_mod_type_row_uses_each_player_speed_mod() {
        let mut row_map = test_row_map(vec![test_row(
            RowId::TypeOfSpeedMod,
            lookup_key("PlayerOptions", "TypeOfSpeedMod"),
            &["x-mod", "c-mod", "m-mod"],
            [2, 2],
        )]);
        let speed_mod = [
            SpeedMod {
                mod_type: SpeedModType::M,
                value: 250.0,
            },
            SpeedMod {
                mod_type: SpeedModType::X,
                value: 2.0,
            },
        ];

        sync_speed_mod_type_row(&mut row_map, &speed_mod);

        assert_eq!(
            row_map
                .get(RowId::TypeOfSpeedMod)
                .unwrap()
                .selected_choice_index,
            [2, 0],
        );
    }

    #[test]
    fn hidden_dropdown_children_anchor_to_parent_row() {
        ensure_i18n();
        for (parent, child, choices, off_idx) in [
            (
                RowId::JudgmentFont,
                RowId::JudgmentOffsetX,
                &["Wendy", "None"][..],
                1,
            ),
            (
                RowId::ComboFont,
                RowId::ComboOffsetX,
                &["Wendy", "None"][..],
                1,
            ),
            (
                RowId::RescoreEarlyHits,
                RowId::EarlyDecentWayOffOptions,
                &["No", "Yes"][..],
                0,
            ),
            (
                RowId::CustomBlueFantasticWindow,
                RowId::CustomBlueFantasticWindowMs,
                &["No", "Yes"][..],
                0,
            ),
            (
                RowId::DataVisualizations,
                RowId::DensityGraphBackground,
                &["Density Graph", "Song Banner", "Judgment Counter"][..],
                0,
            ),
        ] {
            let row_map = test_row_map(vec![
                test_row(
                    parent,
                    lookup_key("PlayerOptions", "JudgmentFont"),
                    choices,
                    [off_idx; 2],
                ),
                test_row(
                    child,
                    lookup_key("PlayerOptions", "JudgmentOffsetX"),
                    &["0"],
                    [0; 2],
                ),
            ]);
            let (_, child_hidden) = hidden_child_f_pos(&row_map);
            assert!(child_hidden, "{child:?} should hide at its parent row");
        }
    }

    #[test]
    fn error_bar_offsets_hide_with_empty_error_bar_mask() {
        ensure_i18n();
        let row_map = test_row_map(vec![
            test_row(
                RowId::ErrorBar,
                lookup_key("PlayerOptions", "ErrorBar"),
                &["Colorful"],
                [0, 0],
            ),
            test_row(
                RowId::ErrorBarOffsetX,
                lookup_key("PlayerOptions", "ErrorBarOffsetX"),
                &["0"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
            ],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::COLORFUL,
                    ..Default::default()
                },
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
            ],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
    }

    #[test]
    fn average_error_bar_children_show_only_for_average_error_bar() {
        ensure_i18n();
        let row_map = test_row_map(vec![
            test_row(
                RowId::ErrorBar,
                lookup_key("PlayerOptions", "ErrorBar"),
                &["Colorful", "Average"],
                [0, 0],
            ),
            test_row(
                RowId::ShortAverageErrorBar,
                lookup_key("PlayerOptions", "ShortAverageErrorBar"),
                &["Off", "On"],
                [0, 0],
            ),
            test_row(
                RowId::CenterTick,
                lookup_key("PlayerOptions", "CenterTick"),
                &["Off", "On"],
                [0, 0],
            ),
            test_row(
                RowId::AverageErrorBarIntensity,
                lookup_key("PlayerOptions", "AverageErrorBarIntensity"),
                &["1.00x", "1.25x"],
                [0, 0],
            ),
            test_row(
                RowId::AverageErrorBarInterval,
                lookup_key("PlayerOptions", "AverageErrorBarInterval"),
                &["100ms", "200ms"],
                [0, 0],
            ),
            test_row(
                RowId::LongErrorBar,
                lookup_key("PlayerOptions", "LongErrorBar"),
                &["Off", "On"],
                [1, 1],
            ),
            test_row(
                RowId::LongErrorBarIntensity,
                lookup_key("PlayerOptions", "LongErrorBarIntensity"),
                &["1.00x", "1.25x"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    error_bar: ErrorBarMask::COLORFUL,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));
        assert!(!is_row_visible(&row_map, 2, visibility));
        assert!(!is_row_visible(&row_map, 3, visibility));
        assert!(!is_row_visible(&row_map, 4, visibility));
        assert!(!is_row_visible(&row_map, 5, visibility));
        assert!(!is_row_visible(&row_map, 6, visibility));

        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    error_bar: ErrorBarMask::AVERAGE,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
        assert!(is_row_visible(&row_map, 2, visibility));
        assert!(is_row_visible(&row_map, 3, visibility));
        assert!(is_row_visible(&row_map, 4, visibility));
        assert!(is_row_visible(&row_map, 5, visibility));
        assert!(is_row_visible(&row_map, 6, visibility));
    }

    #[test]
    fn text_error_bar_children_show_only_for_relevant_text_mode() {
        ensure_i18n();
        let window_row_map = test_row_map(vec![
            test_row(
                RowId::ErrorBar,
                lookup_key("PlayerOptions", "ErrorBar"),
                &["Colorful", "Text"],
                [0, 0],
            ),
            test_row(
                RowId::TextErrorBarMode,
                lookup_key("PlayerOptions", "TextErrorBarMode"),
                &["Window", "Scalable"],
                [0, 0],
            ),
            test_row(
                RowId::TextErrorBarThreshold,
                lookup_key("PlayerOptions", "TextErrorBarThreshold"),
                &["10ms", "11ms"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &window_row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    error_bar: ErrorBarMask::COLORFUL,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(!is_row_visible(&window_row_map, 1, visibility));
        assert!(!is_row_visible(&window_row_map, 2, visibility));

        let visibility = row_visibility(
            &window_row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    error_bar: ErrorBarMask::TEXT,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(is_row_visible(&window_row_map, 1, visibility));
        assert!(!is_row_visible(&window_row_map, 2, visibility));

        let scalable_row_map = test_row_map(vec![
            test_row(
                RowId::ErrorBar,
                lookup_key("PlayerOptions", "ErrorBar"),
                &["Colorful", "Text"],
                [0, 0],
            ),
            test_row(
                RowId::TextErrorBarMode,
                lookup_key("PlayerOptions", "TextErrorBarMode"),
                &["Window", "Scalable"],
                [1, 0],
            ),
            test_row(
                RowId::TextErrorBarThreshold,
                lookup_key("PlayerOptions", "TextErrorBarThreshold"),
                &["10ms", "11ms"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &scalable_row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    error_bar: ErrorBarMask::TEXT,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(is_row_visible(&scalable_row_map, 1, visibility));
        assert!(is_row_visible(&scalable_row_map, 2, visibility));
    }

    #[test]
    fn live_timing_stats_options_hide_until_parent_toggle_active() {
        ensure_i18n();
        let row_map = test_row_map(vec![
            test_row(
                RowId::GameplayExtras,
                lookup_key("PlayerOptions", "GameplayExtras"),
                &["LiveTiming"],
                [0, 0],
            ),
            test_row(
                RowId::LiveTimingStats,
                lookup_key("PlayerOptions", "LiveTimingStats"),
                &["Mean", "MeanAbs", "Max"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    gameplay_extras: GameplayExtrasMask::LIVE_TIMING_STATS,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
    }

    #[test]
    fn column_flash_judgments_hide_until_parent_toggle_active() {
        ensure_i18n();
        let row_map = test_row_map(vec![
            test_row(
                RowId::GameplayExtras,
                lookup_key("PlayerOptions", "GameplayExtras"),
                &["ColumnFlashes"],
                [0, 0],
            ),
            test_row(
                RowId::ColumnFlashJudgments,
                lookup_key("PlayerOptions", "ColumnFlashJudgments"),
                &[
                    "Blue Fantastic",
                    "White Fantastic",
                    "Excellent",
                    "Great",
                    "Decent",
                    "Way Off",
                    "Miss",
                ],
                [0, 0],
            ),
            test_row(
                RowId::ColumnFlashBrightness,
                lookup_key("PlayerOptions", "ColumnFlashBrightness"),
                &["Normal", "Dimmed"],
                [0, 0],
            ),
            test_row(
                RowId::ColumnFlashSize,
                lookup_key("PlayerOptions", "ColumnFlashSize"),
                &["Default", "Compact"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));
        assert!(!is_row_visible(&row_map, 2, visibility));
        assert!(!is_row_visible(&row_map, 3, visibility));

        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    gameplay_extras: GameplayExtrasMask::FLASH_COLUMN_FOR_MISS,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
        assert!(is_row_visible(&row_map, 2, visibility));
        assert!(is_row_visible(&row_map, 3, visibility));
    }

    #[test]
    fn judgment_offsets_hide_when_judgment_font_is_none() {
        ensure_i18n();
        let row_map = test_row_map(vec![
            test_row(
                RowId::JudgmentFont,
                lookup_key("PlayerOptions", "JudgmentFont"),
                &["Love", "None"],
                [1, 0],
            ),
            test_row(
                RowId::JudgmentOffsetX,
                lookup_key("PlayerOptions", "JudgmentOffsetX"),
                &["0"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
            ],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        let row_map = test_row_map(vec![
            test_row(
                RowId::JudgmentFont,
                lookup_key("PlayerOptions", "JudgmentFont"),
                &["Love", "None"],
                [0, 0],
            ),
            test_row(
                RowId::JudgmentOffsetX,
                lookup_key("PlayerOptions", "JudgmentOffsetX"),
                &["0"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
            ],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
    }

    #[test]
    fn combo_offsets_hide_when_all_active_players_use_none_font() {
        ensure_i18n();
        let row_map = test_row_map(vec![
            test_row(
                RowId::ComboFont,
                lookup_key("PlayerOptions", "ComboFont"),
                &["Wendy", "None"],
                [1, 1],
            ),
            test_row(
                RowId::ComboOffsetX,
                lookup_key("PlayerOptions", "ComboOffsetX"),
                &["0"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, true],
            [
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
            ],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        let row_map = test_row_map(vec![
            test_row(
                RowId::ComboFont,
                lookup_key("PlayerOptions", "ComboFont"),
                &["Wendy", "None"],
                [1, 0],
            ),
            test_row(
                RowId::ComboOffsetX,
                lookup_key("PlayerOptions", "ComboOffsetX"),
                &["0"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, true],
            [
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
                PlayerOptionMasks {
                    hide: HideMask::empty(),
                    error_bar: ErrorBarMask::empty(),
                    ..Default::default()
                },
            ],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
    }

    fn target_score_visible_for(row_map: &RowMap) -> bool {
        let visibility = row_visibility(
            row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        is_row_visible(row_map, 1, visibility)
    }

    fn row_id_visible_for(row_map: &RowMap, id: RowId) -> bool {
        let visibility = row_visibility(
            row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        let idx = row_map
            .display_order()
            .iter()
            .position(|&row_id| row_id == id)
            .unwrap_or_else(|| panic!("Row {id:?} missing from test row map"));
        is_row_visible(row_map, idx, visibility)
    }

    #[test]
    fn target_score_hides_until_score_dependent_option_is_active() {
        ensure_i18n();
        let mut row_map = test_row_map(vec![
            test_row(
                RowId::DataVisualizations,
                lookup_key("PlayerOptions", "StepStatistics"),
                &["Density Graph", "Song Banner", "Judgment Counter"],
                [0, 0],
            ),
            test_row(
                RowId::TargetScore,
                lookup_key("PlayerOptions", "TargetScore"),
                &["S"],
                [0, 0],
            ),
            test_row(
                RowId::ActionOnMissedTarget,
                lookup_key("PlayerOptions", "TargetScoreMissPolicy"),
                &["Nothing", "Fail", "Restart"],
                [0, 0],
            ),
            test_row(
                RowId::MiniIndicator,
                lookup_key("PlayerOptions", "MiniIndicator"),
                &[
                    "None",
                    "Subtractive",
                    "Predictive",
                    "Pace",
                    "Rival",
                    "Pacemaker",
                    "StreamProg",
                ],
                [0, 0],
            ),
        ]);

        assert!(!target_score_visible_for(&row_map));

        row_map
            .get_mut(RowId::ActionOnMissedTarget)
            .unwrap()
            .selected_choice_index[P1] = 1;
        assert!(target_score_visible_for(&row_map));

        row_map
            .get_mut(RowId::ActionOnMissedTarget)
            .unwrap()
            .selected_choice_index[P1] = 0;
        row_map
            .get_mut(RowId::MiniIndicator)
            .unwrap()
            .selected_choice_index[P1] = 5;
        assert!(target_score_visible_for(&row_map));
    }

    #[test]
    fn mini_indicator_style_rows_follow_indicator_mode() {
        ensure_i18n();
        let mut row_map = test_row_map(vec![
            test_row(
                RowId::MiniIndicator,
                lookup_key("PlayerOptions", "MiniIndicator"),
                &[
                    "None",
                    "Subtractive",
                    "Predictive",
                    "Pace",
                    "Rival",
                    "Pacemaker",
                    "StreamProg",
                ],
                [0, 0],
            ),
            test_row(
                RowId::IndicatorScoreType,
                lookup_key("PlayerOptions", "IndicatorScoreType"),
                &["ITG", "EX", "H.EX"],
                [0, 0],
            ),
            test_row(
                RowId::MiniIndicatorSubtractiveDisplay,
                lookup_key("PlayerOptions", "MiniIndicatorSubtractiveDisplay"),
                &["Percent", "Points"],
                [0, 0],
            ),
            test_row(
                RowId::MiniIndicatorSize,
                lookup_key("PlayerOptions", "MiniIndicatorSize"),
                &["Default", "Large"],
                [0, 0],
            ),
            test_row(
                RowId::MiniIndicatorColor,
                lookup_key("PlayerOptions", "MiniIndicatorColor"),
                &["Default", "Detailed", "Combo"],
                [0, 0],
            ),
            test_row(
                RowId::MiniIndicatorPosition,
                lookup_key("PlayerOptions", "MiniIndicatorPosition"),
                &["Default", "Under Up Arrow"],
                [0, 0],
            ),
        ]);

        assert!(!row_id_visible_for(&row_map, RowId::IndicatorScoreType));
        assert!(!row_id_visible_for(
            &row_map,
            RowId::MiniIndicatorSubtractiveDisplay
        ));
        assert!(!row_id_visible_for(&row_map, RowId::MiniIndicatorSize));
        assert!(!row_id_visible_for(&row_map, RowId::MiniIndicatorColor));
        assert!(!row_id_visible_for(&row_map, RowId::MiniIndicatorPosition));

        row_map
            .get_mut(RowId::MiniIndicator)
            .unwrap()
            .selected_choice_index[P1] = 1;
        assert!(row_id_visible_for(&row_map, RowId::IndicatorScoreType));
        assert!(row_id_visible_for(
            &row_map,
            RowId::MiniIndicatorSubtractiveDisplay
        ));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorSize));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorColor));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorPosition));

        row_map
            .get_mut(RowId::MiniIndicator)
            .unwrap()
            .selected_choice_index[P1] = 4;
        assert!(row_id_visible_for(&row_map, RowId::IndicatorScoreType));
        assert!(!row_id_visible_for(
            &row_map,
            RowId::MiniIndicatorSubtractiveDisplay
        ));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorSize));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorColor));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorPosition));

        row_map
            .get_mut(RowId::MiniIndicator)
            .unwrap()
            .selected_choice_index[P1] = 6;
        assert!(!row_id_visible_for(&row_map, RowId::IndicatorScoreType));
        assert!(!row_id_visible_for(
            &row_map,
            RowId::MiniIndicatorSubtractiveDisplay
        ));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorSize));
        assert!(!row_id_visible_for(&row_map, RowId::MiniIndicatorColor));
        assert!(row_id_visible_for(&row_map, RowId::MiniIndicatorPosition));
    }

    #[test]
    fn early_dw_options_hide_until_rescore_early_hits_is_active() {
        ensure_i18n();
        let mut row_map = test_row_map(vec![
            test_row(
                RowId::RescoreEarlyHits,
                lookup_key("PlayerOptions", "RescoreEarlyHits"),
                &["No", "Yes"],
                [0, 0],
            ),
            test_row(
                RowId::EarlyDecentWayOffOptions,
                lookup_key("PlayerOptions", "EarlyDecentWayOffOptions"),
                &["Hide Judgments", "Hide Flash"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        row_map
            .get_mut(RowId::RescoreEarlyHits)
            .unwrap()
            .selected_choice_index[P1] = 1;
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
    }

    #[test]
    fn tap_explosion_options_hide_when_skin_is_none() {
        ensure_i18n();
        let mut row_map = test_row_map(vec![
            test_row(
                RowId::TapExplosionSkin,
                lookup_key("PlayerOptions", "TapExplosionSkin"),
                &["Same as NoteSkin", "None", "default"],
                [1, 1],
            ),
            test_row(
                RowId::TapExplosionOptions,
                lookup_key("PlayerOptions", "TapExplosionOptions"),
                &[
                    "Fantastics",
                    "Excellents",
                    "Greats",
                    "Decents",
                    "Way Offs",
                    "Misses",
                    "Held",
                    "Holding",
                ],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        row_map
            .get_mut(RowId::TapExplosionSkin)
            .unwrap()
            .selected_choice_index[P1] = 0;
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
    }

    #[test]
    fn fa_plus_window_options_hide_until_window_is_active() {
        ensure_i18n();
        let row_map = test_row_map(vec![
            test_row(
                RowId::FAPlusOptions,
                lookup_key("PlayerOptions", "FAPlusOptions"),
                &["Window", "EX", "HardEX", "Pane"],
                [0, 0],
            ),
            test_row(
                RowId::FAPlusWindowOptions,
                lookup_key("PlayerOptions", "FAPlusWindowOptions"),
                &["Blue10", "Split"],
                [0, 0],
            ),
        ]);
        let visibility = row_visibility(
            &row_map,
            [true, false],
            [PlayerOptionMasks::default(), PlayerOptionMasks::default()],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks {
                    fa_plus: FaPlusMask::WINDOW,
                    ..Default::default()
                },
                PlayerOptionMasks::default(),
            ],
            false,
        );
        assert!(is_row_visible(&row_map, 1, visibility));
    }

    #[test]
    fn init_active_masks_accumulate_across_panes() {
        // Regression: apply_profile_defaults gates 8 of its 17 returned masks
        // (Scroll, Insert, Remove, Holds, Accel, Effect, Appearance, EarlyDw)
        // on the corresponding row being present in the passed row_map. Those
        // rows live on the Advanced/Uncommon panes, so init() must call the
        // function on all three pane row_maps and OR the resulting masks.
        // Otherwise persisted profile state for those rows is silently lost
        // the moment the user toggles any choice on those rows.
        ensure_i18n();
        let mut profile = Profile::default();
        profile.scroll_option = ScrollOption::Reverse.union(ScrollOption::Cross);

        let mut main_rows = test_row_map(vec![test_row(
            RowId::Exit,
            lookup_key("PlayerOptions", "Exit"),
            &["Exit"],
            [0, 0],
        )]);
        let scroll_binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| {
                    use deadsync_profile::ScrollOption;
                    let mut bits = ScrollMask::empty();
                    if p.scroll_option.contains(ScrollOption::Reverse) {
                        bits.insert(ScrollMask::from_bits_retain(1 << 0));
                    }
                    if p.scroll_option.contains(ScrollOption::Split) {
                        bits.insert(ScrollMask::from_bits_retain(1 << 1));
                    }
                    if p.scroll_option.contains(ScrollOption::Alternate) {
                        bits.insert(ScrollMask::from_bits_retain(1 << 2));
                    }
                    if p.scroll_option.contains(ScrollOption::Cross) {
                        bits.insert(ScrollMask::from_bits_retain(1 << 3));
                    }
                    if p.scroll_option.contains(ScrollOption::Centered) {
                        bits.insert(ScrollMask::from_bits_retain(1 << 4));
                    }
                    bits.bits() as u32
                },
                get_active: |m| m.scroll.bits() as u32,
                set_active: |m, b| {
                    m.scroll = ScrollMask::from_bits_retain(b as u8);
                },
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: TEST_WRITEBACK,
        };
        let mut advanced_rows = test_row_map(vec![test_bitmask_row(
            RowId::Scroll,
            lookup_key("PlayerOptions", "Scroll"),
            &["Reverse", "Split", "Alternate", "Cross", "Centered"],
            scroll_binding,
        )]);
        let mut uncommon_rows = test_row_map(vec![test_row(
            RowId::Exit,
            lookup_key("PlayerOptions", "Exit"),
            &["Exit"],
            [0, 0],
        )]);

        let mut main = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut main_rows, &profile, P1, &mut main);
        // Main alone: Scroll row absent, mask comes back empty (the bug source).
        assert_eq!(main.scroll, ScrollMask::empty());

        // Accumulated across all three panes (the fix): Reverse + Cross preserved.
        let mut combined = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut main_rows, &profile, P1, &mut combined);
        panes::apply_profile_defaults(&mut advanced_rows, &profile, P1, &mut combined);
        panes::apply_profile_defaults(&mut uncommon_rows, &profile, P1, &mut combined);
        assert!(
            combined.scroll.contains(ScrollMask::REVERSE),
            "Reverse bit preserved after in-place accumulation"
        );
        assert!(
            combined.scroll.contains(ScrollMask::CROSS),
            "Cross bit preserved after in-place accumulation"
        );
    }

    /// Regression guard: bitmask rows initialise their cursor to the
    /// position of the first active bit. If a future refactor moves mask
    /// init out of `apply_profile_defaults` (e.g. into a
    /// `BitmaskBinding`-driven table), this behaviour must be preserved.
    #[test]
    fn init_bitmask_row_cursor_starts_at_first_active_bit() {
        ensure_i18n();
        let mut profile = Profile::default();
        // Only the second Hide bit (BACKGROUND, 1 << 1) — cursor must land on
        // choice index 1, not 0.
        profile.hide_targets = false;
        profile.hide_song_bg = true;

        let hide_binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| {
                    let mut bits = HideMask::empty();
                    if p.hide_targets {
                        bits.insert(HideMask::TARGETS);
                    }
                    if p.hide_song_bg {
                        bits.insert(HideMask::BACKGROUND);
                    }
                    if p.hide_combo {
                        bits.insert(HideMask::COMBO);
                    }
                    if p.hide_lifebar {
                        bits.insert(HideMask::LIFE);
                    }
                    if p.hide_score {
                        bits.insert(HideMask::SCORE);
                    }
                    if p.hide_danger {
                        bits.insert(HideMask::DANGER);
                    }
                    if p.hide_combo_explosions {
                        bits.insert(HideMask::COMBO_EXPLOSIONS);
                    }
                    bits.bits() as u32
                },
                get_active: |m| m.hide.bits() as u32,
                set_active: |m, b| {
                    m.hide = HideMask::from_bits_retain(b as u8);
                },
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: TEST_WRITEBACK,
        };
        let mut hide_rows = test_row_map(vec![test_bitmask_row(
            RowId::Hide,
            lookup_key("PlayerOptions", "Hide"),
            &[
                "Targets", "BG", "Combo", "Life", "Score", "Danger", "ComboExp",
            ],
            hide_binding,
        )]);

        let mut masks = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut hide_rows, &profile, P1, &mut masks);

        assert_eq!(
            masks.hide,
            HideMask::BACKGROUND,
            "only BACKGROUND bit should be active",
        );
        let row = hide_rows.get(RowId::Hide).expect("Hide row present");
        assert_eq!(
            row.selected_choice_index[P1], 1,
            "cursor must start at the first active bit (BACKGROUND = index 1)",
        );
    }

    /// Regression guard: `FAPlusOptions` is the lone bitmask row whose
    /// cursor always starts at 0, regardless of which bits are active. Any
    /// data-driven mask-init scheme must preserve this Fixed(0) policy.
    #[test]
    fn init_fa_plus_options_cursor_always_zero() {
        ensure_i18n();
        let mut profile = Profile::default();
        // Activate only the second FA+ bit (EX_SCORE = 1 << 1). Under the
        // generic FirstActiveBit policy the cursor would land on 1; FAPlus
        // pins it to 0.
        profile.show_fa_plus_window = false;
        profile.show_ex_score = true;

        let fa_plus_binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| {
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
                },
                get_active: |m| m.fa_plus.bits() as u32,
                set_active: |m, b| {
                    m.fa_plus = FaPlusMask::from_bits_retain(b as u8);
                },
                cursor: CursorInit::Fixed(0),
            },
            writeback: TEST_WRITEBACK,
        };
        let mut fa_plus_rows = test_row_map(vec![test_bitmask_row(
            RowId::FAPlusOptions,
            lookup_key("PlayerOptions", "FAPlusOptions"),
            &["Window", "EX", "HardEX", "Pane"],
            fa_plus_binding,
        )]);

        let mut masks = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut fa_plus_rows, &profile, P1, &mut masks);

        assert_eq!(
            masks.fa_plus,
            FaPlusMask::EX_SCORE,
            "only EX_SCORE bit should be active",
        );
        let row = fa_plus_rows
            .get(RowId::FAPlusOptions)
            .expect("FAPlusOptions row present");
        assert_eq!(
            row.selected_choice_index[P1], 0,
            "FAPlusOptions cursor must be pinned to 0 even when a non-first bit is active",
        );
    }

    /// Regression guard: `GameplayExtrasMore` is a derived mask with no
    /// constructed Row. Its bits are populated as a side effect of the
    /// `GameplayExtras` profile processing (`column_cues` contributes to
    /// BOTH masks). A row-driven mask registry must explicitly handle this
    /// derivation.
    #[test]
    fn init_gameplay_extras_more_derived_from_sibling_profile_fields() {
        ensure_i18n();
        let mut profile = Profile::default();
        profile.column_cues = true;
        profile.live_timing_stats = true;
        profile.display_scorebox = true;

        // No GameplayExtrasMore row exists (orphan; see the
        // `every_row_id_is_constructed_by_some_pane` test) — we still expect
        // the derived mask bits to be populated.
        let gameplay_extras_binding = BitmaskBinding::Generic {
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
                    if p.live_timing_stats {
                        bits.insert(GameplayExtrasMask::LIVE_TIMING_STATS);
                    }
                    if p.display_scorebox {
                        bits.insert(GameplayExtrasMask::DISPLAY_SCOREBOX);
                    }
                    bits.bits() as u32
                },
                get_active: |m| m.gameplay_extras.bits() as u32,
                set_active: |m, b| {
                    m.gameplay_extras = GameplayExtrasMask::from_bits_retain(b as u16);
                },
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: TEST_WRITEBACK,
        };
        let mut rows = test_row_map(vec![test_bitmask_row(
            RowId::GameplayExtras,
            lookup_key("PlayerOptions", "GameplayExtras"),
            &[
                "FlashMiss",
                "DensityTop",
                "ColumnCues",
                "LiveTiming",
                "DisplayScorebox",
            ],
            gameplay_extras_binding,
        )]);

        let mut masks = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut rows, &profile, P1, &mut masks);

        assert!(
            masks
                .gameplay_extras
                .contains(GameplayExtrasMask::COLUMN_CUES),
            "GameplayExtras COLUMN_CUES bit set from profile",
        );
        assert!(
            masks
                .gameplay_extras
                .contains(GameplayExtrasMask::LIVE_TIMING_STATS),
            "GameplayExtras LIVE_TIMING_STATS bit set from profile",
        );
        assert!(
            masks
                .gameplay_extras
                .contains(GameplayExtrasMask::DISPLAY_SCOREBOX),
            "GameplayExtras DISPLAY_SCOREBOX bit set from profile",
        );
        assert!(
            masks
                .gameplay_extras_more
                .contains(GameplayExtrasMoreMask::COLUMN_CUES),
            "derived GameplayExtrasMore COLUMN_CUES bit set from sibling profile field",
        );
        assert!(
            masks
                .gameplay_extras_more
                .contains(GameplayExtrasMoreMask::DISPLAY_SCOREBOX),
            "derived GameplayExtrasMore DISPLAY_SCOREBOX bit set from sibling profile field",
        );
    }

    #[test]
    fn hud_offset_choices_cover_full_range() {
        let choices = hud_offset_choices();
        assert_eq!(choices.first().map(String::as_str), Some("-250"));
        assert_eq!(
            choices.get(HUD_OFFSET_ZERO_INDEX).map(String::as_str),
            Some("0")
        );
        assert_eq!(choices.last().map(String::as_str), Some("250"));
        assert_eq!(choices.len() as i32, HUD_OFFSET_MAX - HUD_OFFSET_MIN + 1);
    }

    #[test]
    fn held_arcade_start_keeps_advancing_rows() {
        ensure_i18n();
        let song = test_song();

        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);

        let mut asset_manager = AssetManager::new();
        register_test_fonts(&mut asset_manager);

        let mut state = super::init(
            song,
            [0; 2],
            [0; 2],
            1,
            Screen::SelectMusic,
            None,
            test_noteskin_catalog(),
            deadsync_theme::views::SmxGifCatalogView::default(),
            super::HeartRateDevicesView::default(),
        );
        let active = session_active_players();
        let first_row = state.pane().selected_row[P1];
        assert!(handle_arcade_start_event(&mut state, &asset_manager, active, P1).is_none());
        let second_row = state.pane().selected_row[P1];
        assert!(second_row > first_row);

        on_start_press(&mut state, P1);
        assert!(
            repeat_held_arcade_start(
                &mut state,
                &asset_manager,
                active,
                P1,
                (NAV_INITIAL_HOLD_DELAY + Duration::from_millis(1)).as_secs_f32(),
            )
            .is_none()
        );
        assert!(state.pane().selected_row[P1] > second_row);
    }

    #[test]
    fn held_arcade_start_stops_at_exit_row() {
        ensure_i18n();
        let song = test_song();

        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);

        let mut asset_manager = AssetManager::new();
        register_test_fonts(&mut asset_manager);

        let mut state = super::init(
            song,
            [0; 2],
            [0; 2],
            1,
            Screen::SelectMusic,
            None,
            test_noteskin_catalog(),
            deadsync_theme::views::SmxGifCatalogView::default(),
            super::HeartRateDevicesView::default(),
        );
        let active = session_active_players();
        let last_row = state.pane().row_map.len().saturating_sub(1);
        state.pane_mut().selected_row[P1] = last_row;
        state.pane_mut().prev_selected_row[P1] = last_row;

        on_start_press(&mut state, P1);
        assert!(
            repeat_held_arcade_start(
                &mut state,
                &asset_manager,
                active,
                P1,
                (NAV_INITIAL_HOLD_DELAY + Duration::from_millis(1)).as_secs_f32(),
            )
            .is_none()
        );
        assert_eq!(state.pane().selected_row[P1], last_row);
    }

    fn setup_state() -> (super::State, AssetManager) {
        let song = test_song();
        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);
        let mut asset_manager = AssetManager::new();
        register_test_fonts(&mut asset_manager);
        let state = super::init(
            song,
            [0; 2],
            [0; 2],
            1,
            Screen::SelectMusic,
            None,
            test_noteskin_catalog(),
            deadsync_theme::views::SmxGifCatalogView::default(),
            super::HeartRateDevicesView::default(),
        );
        (state, asset_manager)
    }

    fn setup_versus_state() -> (super::State, AssetManager) {
        let song = test_song();
        profile::set_session_play_style(PlayStyle::Versus);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, true);
        let mut asset_manager = AssetManager::new();
        register_test_fonts(&mut asset_manager);
        let state = super::init(
            song,
            [0; 2],
            [0; 2],
            1,
            Screen::SelectMusic,
            None,
            test_noteskin_catalog(),
            deadsync_theme::views::SmxGifCatalogView::default(),
            super::HeartRateDevicesView::default(),
        );
        (state, asset_manager)
    }

    #[test]
    fn heart_rate_choices_keep_saved_devices_that_are_not_broadcasting() {
        ensure_i18n();
        let mut p1 = Profile::default();
        p1.heart_rate_device_id = Some("saved-id".to_owned());
        let profiles = [p1, Profile::default()];
        let devices = super::HeartRateDevicesView {
            supported: true,
            scanning: true,
            devices: vec![super::HeartRateDeviceView {
                id: "nearby-id".to_owned(),
                label: "Nearby HRM".to_owned(),
            }],
            error: None,
            readings: [super::HeartRateReadingView::default(); 2],
        };

        let (choices, ids) = super::heart_rate_choices(&devices, &profiles);

        assert_eq!(
            ids,
            vec![
                None,
                Some("nearby-id".to_owned()),
                Some("saved-id".to_owned())
            ]
        );
        assert_eq!(choices[1], "Nearby HRM");
        assert_eq!(choices[2], "Saved HRM");
        assert!(choices.iter().all(|choice| !choice.contains("saved-id")));
    }

    fn assert_sfx(effect: &ThemeEffect, expected_path: &str) {
        assert!(matches!(
            effect,
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::PlaySfx(path)
            )) if path == expected_path
        ));
    }

    #[test]
    fn queued_audio_requests_precede_navigation() {
        ensure_i18n();
        let (mut state, _) = setup_state();
        queue_audio(&mut state, AudioRequest::SetMusicRate(1.25));
        queue_sfx(&mut state, "assets/sounds/change_value.ogg");
        queue_sfx(&mut state, "assets/sounds/start.ogg");

        let effect = prepend_pending_audio(&mut state, ThemeEffect::Navigate(Screen::Gameplay));
        let ThemeEffect::Batch(effects) = effect else {
            panic!("queued Player Options audio should precede navigation");
        };
        assert_eq!(effects.len(), 4);
        assert!(matches!(
            effects[0],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::SetMusicRate(rate)
            )) if rate == 1.25
        ));
        assert_sfx(&effects[1], "assets/sounds/change_value.ogg");
        assert_sfx(&effects[2], "assets/sounds/start.ogg");
        assert!(matches!(
            effects[3],
            ThemeEffect::Navigate(Screen::Gameplay)
        ));
        assert!(state.pending_audio.is_empty());
    }

    #[test]
    fn music_rate_request_precedes_change_sfx() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        let rate_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::MusicRate)
            .expect("Music Rate should be in Main pane");
        state.pane_mut().selected_row[P1] = rate_row;
        state.pane_mut().prev_selected_row[P1] = rate_row;
        let before = state.music_rate;

        handle_nav_event(
            &mut state,
            &asset_manager,
            session_active_players(),
            P1,
            NavDirection::Right,
            true,
        );
        let effect = update(&mut state, 0.0, &asset_manager)
            .expect("music-rate change should emit ordered audio work");
        let ThemeEffect::Batch(effects) = effect else {
            panic!("rate update and change cue should be batched");
        };
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            effects[0],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::SetMusicRate(rate)
            )) if rate == state.music_rate && rate > before
        ));
        assert_sfx(&effects[1], "assets/sounds/change_value.ogg");
    }

    #[test]
    fn held_speed_mod_repeat_uses_update_dt() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        let active = session_active_players();
        let speed_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::SpeedMod)
            .expect("Speed Mod should be in Main pane");
        state.pane_mut().selected_row[P1] = speed_row;
        state.pane_mut().prev_selected_row[P1] = speed_row;

        let before = state.speed_mod[P1].value;
        handle_nav_event(
            &mut state,
            &asset_manager,
            active,
            P1,
            NavDirection::Right,
            true,
        );
        let after_press = state.speed_mod[P1].value;
        assert!(after_press > before);

        let press_effect = update(&mut state, 0.0, &asset_manager)
            .expect("initial choice change should emit its queued sound");
        assert_sfx(&press_effect, "assets/sounds/change_value.ogg");
        assert_eq!(state.speed_mod[P1].value, after_press);

        let repeat_effect = update(
            &mut state,
            (NAV_INITIAL_HOLD_DELAY + Duration::from_millis(1)).as_secs_f32(),
            &asset_manager,
        )
        .expect("held choice repeat should emit its queued sound");
        assert_sfx(&repeat_effect, "assets/sounds/change_value.ogg");
        assert!(state.speed_mod[P1].value > after_press);
    }

    #[test]
    fn p2_speed_row_uses_p2_option_column() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_versus_state();
        state.speed_mod[P1] = SpeedMod {
            mod_type: SpeedModType::M,
            value: 690.0,
        };
        state.speed_mod[P2] = SpeedMod {
            mod_type: SpeedModType::M,
            value: 250.0,
        };

        let speed_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::SpeedMod)
            .expect("Speed Mod should be in Main pane");
        state.pane_mut().selected_row[P2] = speed_row;
        let row_y = state.pane().row_tweens[speed_row].y();
        let expected_x = player_option_column_x(P2);

        let (cursor_x, _, _, _) =
            super::cursor_dest_for_player(&state, &asset_manager, P2).unwrap();
        assert!(
            (cursor_x - expected_x).abs() < 0.01,
            "P2 cursor should use the P2 option column"
        );

        let actors = super::get_actors(&state, &asset_manager);
        let p2_text_x = actors.iter().find_map(|actor| match actor {
            deadlib_present::actors::Actor::Text {
                offset, content, z, ..
            } if *z == super::Z_ROW_FOREGROUND
                && content.as_str() == "M250"
                && (offset[1] - row_y).abs() < 0.01 =>
            {
                Some(offset[0])
            }
            _ => None,
        });
        let p2_text_x = p2_text_x.expect("P2 Speed Mod row text should render");
        assert!(
            (p2_text_x - expected_x).abs() < 0.01,
            "P2 Speed Mod row text should use the P2 option column"
        );
    }

    #[test]
    fn player_options_keeps_header_without_footer() {
        ensure_i18n();
        let (state, asset_manager) = setup_state();
        let actors = super::get_actors(&state, &asset_manager);

        let is_screen_bar = |actor: &deadlib_present::actors::Actor, bottom: bool| {
            let deadlib_present::actors::Actor::Frame {
                align,
                offset,
                size,
                z,
                ..
            } = actor
            else {
                return false;
            };
            let deadlib_present::actors::SizeSpec::Px(h) = size[1] else {
                return false;
            };
            let y_matches = if bottom {
                (align[1] - 1.0).abs() < 0.001
                    && (offset[1] - deadlib_present::space::screen_height()).abs() < 0.001
            } else {
                align[1].abs() < 0.001 && offset[1].abs() < 0.001
            };
            *z == 120 && (h - 32.0).abs() < 0.001 && y_matches
        };

        assert!(
            actors.iter().any(|actor| is_screen_bar(actor, false)),
            "ScreenPlayerOptions should keep the header"
        );
        assert!(
            !actors.iter().any(|actor| is_screen_bar(actor, true)),
            "ScreenPlayerOptions metrics hide the footer"
        );
    }

    #[test]
    fn versus_shared_cursor_rings_stack_by_player() {
        ensure_i18n();
        let (mut state, _asset_manager) = setup_versus_state();

        let exit_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::Exit)
            .expect("Exit should be in Main pane");
        state.pane_mut().selected_row = [exit_row, exit_row];

        let rect = super::CursorRect::new(100.0, 50.0, 40.0, 20.0);
        let pane = state.pane_mut();
        pane.cursor_initialized = [true, true];
        pane.cursor_from = [rect, rect];
        pane.cursor_to = [rect, rect];
        pane.cursor_t = [1.0, 1.0];

        let mut actors = Vec::new();
        super::draw_cursor_ring(&mut actors, &state, [true, true], exit_row, 1.0);
        assert_eq!(actors.len(), 8, "two 4-sided cursor rings should draw");

        let sprite_y = |idx: usize| match &actors[idx] {
            deadlib_present::actors::Actor::Sprite { offset, .. } => offset[1],
            _ => panic!("cursor ring actor should be a quad sprite"),
        };
        let p1_top_y = sprite_y(0);
        let p2_top_y = sprite_y(4);
        assert!(
            p1_top_y < p2_top_y,
            "Arrow Cloud metrics place P1 one pixel above P2"
        );
        assert!(
            (p2_top_y - p1_top_y - 2.0).abs() < 0.001,
            "P1/P2 cursor centers should differ by two pixels"
        );
    }

    #[test]
    fn dispatch_with_zero_delta_commits_choice() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::BackgroundFilter)
            .expect("BackgroundFilter should be in Main pane");

        // Pre-set to Off (index 0) so we can detect a write
        state
            .pane_mut()
            .row_map
            .get_mut(RowId::BackgroundFilter)
            .unwrap()
            .selected_choice_index[P1] = 0;
        state.player_profiles[P1].background_filter = BackgroundFilter::from_percent(95);
        state.pane_mut().selected_row[P1] = row_index;

        // delta=0 should still apply the current choice
        super::change_choice_for_player(&mut state, &asset_manager, P1, 0, super::NavWrap::Wrap);

        assert_eq!(
            state.player_profiles[P1].background_filter,
            BackgroundFilter::OFF,
            "delta=0 must apply the current selected index to the profile"
        );
    }

    #[test]
    fn dispatch_what_comes_next_cycles_and_mirrors() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::WhatComesNext)
            .expect("WhatComesNext should be in Main pane");

        state.pane_mut().selected_row[P1] = row_index;
        let initial = state
            .pane()
            .row_map
            .get(RowId::WhatComesNext)
            .unwrap()
            .selected_choice_index[P1];

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);

        let row = state.pane().row_map.get(RowId::WhatComesNext).unwrap();
        let n = row.choices.len();
        let expected = (initial + 1) % n;
        assert_eq!(
            row.selected_choice_index[0], expected,
            "P1 slot should advance"
        );
        assert_eq!(
            row.selected_choice_index[1], expected,
            "P2 slot should mirror"
        );
    }

    #[test]
    fn dispatch_bitmask_via_toggle() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        // Inline Scroll binding (toggle-capable) for this dispatch test.
        // Mirrors the shape of the production SCROLL binding without
        // taking a dependency on the panes module's private constants.
        let scroll_binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |_| 0,
                get_active: |m| m.scroll.bits() as u32,
                set_active: |m, b| {
                    m.scroll = ScrollMask::from_bits_retain(b as u8);
                },
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, _, _| {},
                persist_for_side: |_, _| {},
                bit_mapping: BitMapping::Sequential { width: 5 },
                sync_visibility: false,
            },
        };
        let scroll_row = Row {
            id: RowId::Scroll,
            behavior: super::RowBehavior::Bitmask(scroll_binding),
            name: lookup_key("PlayerOptions", "Scroll"),
            choices: ["Reverse", "Split", "Alternate", "Cross", "Centered"]
                .iter()
                .map(ToString::to_string)
                .collect(),
            selected_choice_index: [0, 0],
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        };
        state.pane_mut().row_map.display_order.push(RowId::Scroll);
        state.pane_mut().row_map.insert(scroll_row);
        let row_index = state.pane().row_map.display_order().len() - 1;
        state.pane_mut().selected_row[P1] = row_index;
        state.option_masks[P1].scroll = ScrollMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_ne!(
            state.option_masks[P1].scroll,
            ScrollMask::empty(),
            "Scroll bitmask should have been toggled"
        );
    }

    #[test]
    fn dispatch_judgment_tilt_marks_visibility_change() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        // Insert JudgmentTilt and JudgmentTiltIntensity into the row_map.
        let tilt_binding = super::ChoiceBinding::<bool> {
            apply: |p, v| {
                p.judgment_tilt = v;
                super::Outcome::persisted_with_visibility()
            },
            persist_for_side: profile::update_judgment_tilt_for_side,
            init: None,
        };
        let tilt_row = Row {
            id: RowId::JudgmentTilt,
            behavior: super::RowBehavior::Cycle(super::CycleBinding::Bool(tilt_binding)),
            name: lookup_key("PlayerOptions", "JudgmentTilt"),
            choices: ["No", "Yes"].iter().map(ToString::to_string).collect(),
            selected_choice_index: [0, 0],
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        };
        let tilt_intensity_row = test_row(
            RowId::JudgmentTiltIntensity,
            lookup_key("PlayerOptions", "JudgmentTiltIntensity"),
            &["1.0", "2.0"],
            [0, 0],
        );
        state
            .pane_mut()
            .row_map
            .display_order
            .push(RowId::JudgmentTilt);
        state.pane_mut().row_map.insert(tilt_row);
        state
            .pane_mut()
            .row_map
            .display_order
            .push(RowId::JudgmentTiltIntensity);
        state.pane_mut().row_map.insert(tilt_intensity_row);

        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::JudgmentTilt)
            .unwrap();
        state.pane_mut().selected_row[P1] = row_index;

        let active = session_active_players();
        // Initially JudgmentTilt=0 (off) so JudgmentTiltIntensity should be hidden.
        assert!(
            !judgment_tilt_options_visible(&state.pane().row_map, active),
            "JudgmentTiltIntensity should start hidden"
        );

        // Advance to index 1 (enabled) — apply returns persisted_with_visibility → syncs
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);

        assert!(
            judgment_tilt_options_visible(&state.pane().row_map, active),
            "JudgmentTiltIntensity should be visible after enabling JudgmentTilt"
        );
    }

    #[test]
    fn dispatch_cycle_index_advances_per_player_only() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::BackgroundFilter)
            .expect("BackgroundFilter should be in Main pane");

        // Pin both slots at index 0 so a P1 advance is unambiguously detectable.
        let row = state
            .pane_mut()
            .row_map
            .get_mut(RowId::BackgroundFilter)
            .unwrap();
        row.selected_choice_index = [0, 0];
        let n = row.choices.len();
        assert!(n >= 2, "BackgroundFilter should have at least 2 choices");
        state.pane_mut().selected_row[P1] = row_index;

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);

        let row = state.pane().row_map.get(RowId::BackgroundFilter).unwrap();
        assert_eq!(row.selected_choice_index[0], 1, "P1 should have advanced");
        assert_eq!(
            row.selected_choice_index[1], 0,
            "non-mirrored Numeric must not touch P2's slot"
        );
    }

    #[test]
    fn dispatch_wraps_at_choice_bounds() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::BackgroundFilter)
            .expect("BackgroundFilter should be in Main pane");
        state.pane_mut().selected_row[P1] = row_index;

        let n = state
            .pane()
            .row_map
            .get(RowId::BackgroundFilter)
            .unwrap()
            .choices
            .len();
        assert!(n >= 2, "wrap test needs at least 2 choices");

        // Forward wrap: last → 0
        state
            .pane_mut()
            .row_map
            .get_mut(RowId::BackgroundFilter)
            .unwrap()
            .selected_choice_index[P1] = n - 1;
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);
        assert_eq!(
            state
                .pane()
                .row_map
                .get(RowId::BackgroundFilter)
                .unwrap()
                .selected_choice_index[P1],
            0,
            "delta=+1 at last index should wrap to 0"
        );

        // Backward wrap: 0 → last
        state
            .pane_mut()
            .row_map
            .get_mut(RowId::BackgroundFilter)
            .unwrap()
            .selected_choice_index[P1] = 0;
        super::change_choice_for_player(&mut state, &asset_manager, P1, -1, super::NavWrap::Wrap);
        assert_eq!(
            state
                .pane()
                .row_map
                .get(RowId::BackgroundFilter)
                .unwrap()
                .selected_choice_index[P1],
            n - 1,
            "delta=-1 at index 0 should wrap to last"
        );
    }

    #[test]
    fn dispatch_on_exit_action_is_no_op_for_delta() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::Exit)
            .expect("Exit should be in Main pane");
        state.pane_mut().selected_row[P1] = row_index;

        let before = state
            .pane()
            .row_map
            .get(RowId::Exit)
            .unwrap()
            .selected_choice_index;

        // RowBehavior::Exit returns Outcome::NONE so the dispatcher must not panic,
        // mutate the row, or play SFX (which would panic — audio uninit in tests).
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);
        super::change_choice_for_player(&mut state, &asset_manager, P1, -3, super::NavWrap::Wrap);

        let after = state
            .pane()
            .row_map
            .get(RowId::Exit)
            .unwrap()
            .selected_choice_index;
        assert_eq!(
            before, after,
            "RowBehavior::Exit must not advance its own choice index"
        );
    }

    #[test]
    fn versus_exit_requires_both_players_on_exit_row() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_versus_state();

        let exit_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::Exit)
            .expect("Exit should be in Main pane");
        assert!(exit_row > 0, "Exit should follow WhatComesNext");
        let other_row = exit_row - 1;
        state.pane_mut().selected_row[P1] = exit_row;
        state.pane_mut().selected_row[P2] = other_row;

        let active = session_active_players();
        assert_eq!(
            active,
            [true, true],
            "versus setup should activate both players"
        );

        let action = handle_start_event(&mut state, &asset_manager, active, P1);
        assert!(
            matches!(action, None),
            "ITG parity: pressing Exit in versus is a no-op until both players are on the last row"
        );
        assert_eq!(state.pane().selected_row[P1], exit_row);
        assert_eq!(state.pane().selected_row[P2], other_row);
    }

    #[test]
    fn versus_exit_navigates_once_both_players_are_on_exit_row() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_versus_state();

        let exit_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::Exit)
            .expect("Exit should be in Main pane");
        state.pane_mut().selected_row = [exit_row, exit_row];

        let active = session_active_players();
        let action = handle_start_event(&mut state, &asset_manager, active, P2);
        assert!(
            matches!(action, Some(ThemeEffect::Navigate(Screen::Gameplay))),
            "once both players are on Exit, either player should be able to leave the screen"
        );
    }

    #[test]
    fn practice_exit_starts_practice_from_player_options() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        state.return_screen = Screen::Practice;

        let exit_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::Exit)
            .expect("Exit should be in Main pane");
        state.pane_mut().selected_row[P1] = exit_row;

        let active = [true, false];
        let action = handle_start_event(&mut state, &asset_manager, active, P1);
        assert!(
            matches!(action, Some(ThemeEffect::Navigate(Screen::Practice))),
            "practice-launched player options should start Practice, not Gameplay"
        );
    }

    #[test]
    fn practice_choose_different_returns_to_select_music() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        state.return_screen = Screen::Practice;

        let exit_row = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::Exit)
            .expect("Exit should be in Main pane");
        state.pane_mut().selected_row[P1] = exit_row;
        state
            .pane_mut()
            .row_map
            .get_mut(RowId::WhatComesNext)
            .unwrap()
            .selected_choice_index[P1] = 1;

        let active = [true, false];
        let action = handle_start_event(&mut state, &asset_manager, active, P1);
        assert!(
            matches!(action, Some(ThemeEffect::Navigate(Screen::SelectMusic))),
            "choose different song from practice options should return to the wheel"
        );
    }

    #[test]
    fn what_comes_next_indices_route_without_localized_text() {
        for (current, choice, target) in [
            (OptionsPane::Main, 2, OptionsPane::Display),
            (OptionsPane::Main, 3, OptionsPane::Advanced),
            (OptionsPane::Main, 4, OptionsPane::Uncommon),
            (OptionsPane::Display, 2, OptionsPane::Main),
            (OptionsPane::Display, 3, OptionsPane::Advanced),
            (OptionsPane::Display, 4, OptionsPane::Uncommon),
            (OptionsPane::Advanced, 2, OptionsPane::Main),
            (OptionsPane::Advanced, 3, OptionsPane::Display),
            (OptionsPane::Advanced, 4, OptionsPane::Uncommon),
            (OptionsPane::Uncommon, 2, OptionsPane::Main),
            (OptionsPane::Uncommon, 3, OptionsPane::Display),
            (OptionsPane::Uncommon, 4, OptionsPane::Advanced),
        ] {
            assert_eq!(what_comes_next_pane(current, choice), Some(target));
        }
        assert_eq!(what_comes_next_pane(OptionsPane::Main, 0), None);
        assert_eq!(what_comes_next_pane(OptionsPane::Main, 1), None);
        assert_eq!(what_comes_next_pane(OptionsPane::Main, 5), None);
    }

    #[test]
    fn dispatch_on_bitmask_via_delta_is_no_op() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        // Insert a Bitmask row (Scroll lives in the Advanced pane, so attach it
        // to the Main row_map directly for this isolated test).
        let scroll_binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |_| 0,
                get_active: |m| m.scroll.bits() as u32,
                set_active: |m, b| {
                    m.scroll = ScrollMask::from_bits_retain(b as u8);
                },
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, _, _| {},
                persist_for_side: |_, _| {},
                bit_mapping: BitMapping::Sequential { width: 5 },
                sync_visibility: false,
            },
        };
        let scroll_row = Row {
            id: RowId::Scroll,
            behavior: super::RowBehavior::Bitmask(scroll_binding),
            name: lookup_key("PlayerOptions", "Scroll"),
            choices: ["Reverse", "Split", "Alternate", "Cross", "Centered"]
                .iter()
                .map(ToString::to_string)
                .collect(),
            selected_choice_index: [2, 0],
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        };
        state.pane_mut().row_map.display_order.push(RowId::Scroll);
        state.pane_mut().row_map.insert(scroll_row);
        let row_index = state.pane().row_map.display_order().len() - 1;
        state.pane_mut().selected_row[P1] = row_index;
        state.option_masks[P1].scroll = ScrollMask::empty();
        state.option_masks[P2].scroll = ScrollMask::empty();

        // L/R on a bitmask row returns Outcome::NONE — mask must not change,
        // and no SFX should be played (audio uninit in tests would panic).
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);
        super::change_choice_for_player(&mut state, &asset_manager, P1, -1, super::NavWrap::Wrap);

        assert_eq!(
            [state.option_masks[P1].scroll, state.option_masks[P2].scroll],
            [ScrollMask::empty(), ScrollMask::empty()],
            "delta on Bitmask row must not toggle the mask"
        );
        // selected_choice_index is also untouched (cycle_choice_index never runs)
        assert_eq!(
            state
                .pane()
                .row_map
                .get(RowId::Scroll)
                .unwrap()
                .selected_choice_index,
            [2, 0],
            "Bitmask delta must not advance the row's selected_choice_index either"
        );
    }

    fn build_all_pane_row_maps(state: &super::State) -> Vec<(super::OptionsPane, RowMap)> {
        let noteskin_names = test_noteskin_catalog().names;
        let heart_rate_choices = vec!["Off".to_owned()];
        [
            super::OptionsPane::Main,
            super::OptionsPane::Display,
            super::OptionsPane::Advanced,
            super::OptionsPane::Uncommon,
        ]
        .iter()
        .map(|&pane| {
            let map = super::build_rows(
                &state.song,
                &state.speed_mod[P1],
                state.chart_steps_index,
                [0; 2],
                state.music_rate,
                pane,
                &noteskin_names,
                &[],
                &[],
                &heart_rate_choices,
                Screen::SelectMusic,
                state.fixed_stepchart.as_ref(),
            );
            (pane, map)
        })
        .collect()
    }

    fn advanced_step_statistics_choices(
        state: &super::State,
        return_screen: Screen,
    ) -> Vec<String> {
        let noteskin_names = test_noteskin_catalog().names;
        let row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Advanced,
            &noteskin_names,
            &[],
            &[],
            &[],
            return_screen,
            state.fixed_stepchart.as_ref(),
        );
        row_map
            .get(RowId::DataVisualizations)
            .expect("Step Statistics row present")
            .choices
            .clone()
    }

    #[test]
    fn course_options_label_pack_info_as_course_banner() {
        ensure_i18n();
        let (state, _asset_manager) = setup_state();

        let normal = advanced_step_statistics_choices(&state, Screen::SelectMusic);
        let course = advanced_step_statistics_choices(&state, Screen::SelectCourse);

        assert_eq!(
            normal[4],
            crate::assets::i18n::tr("PlayerOptions", "StepStatisticsPackInfo").as_ref()
        );
        assert_eq!(
            course[4],
            crate::assets::i18n::tr("PlayerOptions", "StepStatisticsCourseBanner").as_ref()
        );
    }

    #[test]
    fn display_pane_owns_display_rows_and_main_keeps_shared_rows() {
        ensure_i18n();
        let (state, _asset_manager) = setup_state();
        let maps = build_all_pane_row_maps(&state);
        let row_map_for = |pane| {
            maps.iter()
                .find(|(p, _)| *p == pane)
                .map(|(_, map)| map)
                .unwrap()
        };
        let main = row_map_for(super::OptionsPane::Main);
        let display = row_map_for(super::OptionsPane::Display);
        let advanced = row_map_for(super::OptionsPane::Advanced);

        for id in [
            RowId::Mini,
            RowId::Perspective,
            RowId::NoteSkin,
            RowId::JudgmentFont,
            RowId::ComboFont,
            RowId::HoldJudgment,
            RowId::HeldGraphic,
            RowId::BackgroundFilter,
        ] {
            assert!(main.get(id).is_some(), "{id:?} should remain in Main");
            assert!(display.get(id).is_some(), "{id:?} should be in Display");
        }

        for id in [
            RowId::Spacing,
            RowId::MineSkin,
            RowId::ReceptorSkin,
            RowId::TapExplosionSkin,
            RowId::TapExplosionOptions,
            RowId::JudgmentOffsetX,
            RowId::JudgmentOffsetY,
            RowId::ComboOffsetX,
            RowId::ComboOffsetY,
            RowId::NoteFieldOffsetX,
            RowId::NoteFieldOffsetY,
        ] {
            assert!(main.get(id).is_none(), "{id:?} should move out of Main");
            assert!(display.get(id).is_some(), "{id:?} should be in Display");
        }

        for id in [
            RowId::CenterTick,
            RowId::AverageErrorBarIntensity,
            RowId::AverageErrorBarInterval,
        ] {
            assert!(main.get(id).is_none(), "{id:?} should stay out of Main");
            assert!(advanced.get(id).is_some(), "{id:?} should be in Advanced");
        }
    }

    #[test]
    fn main_what_comes_next_lists_display_before_advanced() {
        ensure_i18n();
        let choices = panes::what_comes_next_choices(super::OptionsPane::Main, Screen::SelectMusic);
        assert_eq!(
            choices,
            vec![
                crate::assets::i18n::tr("PlayerOptions", "WhatComesNextGameplay").to_string(),
                crate::assets::i18n::tr("PlayerOptions", "ChooseDifferentSong").to_string(),
                crate::assets::i18n::tr("PlayerOptions", "WhatComesNextDisplayModifiers")
                    .to_string(),
                crate::assets::i18n::tr("PlayerOptions", "WhatComesNextAdvancedModifiers")
                    .to_string(),
                crate::assets::i18n::tr("PlayerOptions", "WhatComesNextUncommonModifiers")
                    .to_string(),
            ],
        );
    }

    #[test]
    fn pane_switch_refreshes_shared_row_defaults() {
        ensure_i18n();
        let (mut state, _asset_manager) = setup_state();
        state.player_profiles[P1].mini_percent = 37;
        state.panes[super::OptionsPane::Display.index()]
            .row_map
            .get_mut(RowId::Mini)
            .unwrap()
            .selected_choice_index[P1] = 0;

        super::apply_pane(&mut state, super::OptionsPane::Display);

        assert_choice_at_cursor(&state.pane().row_map, RowId::Mini, "37%");
    }

    #[test]
    fn pane_switch_resets_what_comes_next_to_gameplay() {
        ensure_i18n();
        let (mut state, _asset_manager) = setup_state();
        let gameplay = crate::assets::i18n::tr("PlayerOptions", "WhatComesNextGameplay");

        for pane in [
            super::OptionsPane::Display,
            super::OptionsPane::Advanced,
            super::OptionsPane::Uncommon,
        ] {
            let row = state.panes[pane.index()]
                .row_map
                .get_mut(RowId::WhatComesNext)
                .unwrap();
            row.selected_choice_index = [2, 2];

            super::apply_pane(&mut state, pane);

            let row = state.pane().row_map.get(RowId::WhatComesNext).unwrap();
            for player_idx in [P1, P2] {
                assert_eq!(row.selected_choice_index[player_idx], 0);
                assert_eq!(
                    row.choices[row.selected_choice_index[player_idx]],
                    gameplay.as_ref()
                );
            }
        }
    }

    #[test]
    fn every_built_row_has_consistent_choices_and_index() {
        ensure_i18n();
        let (state, _asset_manager) = setup_state();

        for (pane, row_map) in build_all_pane_row_maps(&state) {
            for &id in row_map.display_order() {
                let row = row_map.get(id).unwrap_or_else(|| {
                    panic!("display_order references {id:?} in {pane:?} but no row stored")
                });
                assert!(
                    !row.choices.is_empty(),
                    "{pane:?}/{id:?}: row has no choices",
                );
                for (slot, &idx) in row.selected_choice_index.iter().enumerate() {
                    assert!(
                        idx < row.choices.len(),
                        "{pane:?}/{id:?}: selected_choice_index[{slot}]={idx} out of bounds (len={})",
                        row.choices.len(),
                    );
                }
            }
        }
    }

    #[test]
    fn every_rowid_is_built_in_some_pane() {
        ensure_i18n();
        let (state, _asset_manager) = setup_state();

        let mut built = [false; RowId::COUNT];
        for (_pane, row_map) in build_all_pane_row_maps(&state) {
            for &id in row_map.display_order() {
                built[id as usize] = true;
            }
        }

        // RowId is #[repr(usize)] with sequential discriminants 0..COUNT, so
        // we can enumerate every variant by transmute. Sound because every
        // value in 0..COUNT corresponds to a defined RowId variant.
        for i in 0..RowId::COUNT {
            let id: RowId = unsafe { std::mem::transmute::<usize, RowId>(i) };
            // Known orphan: defined and referenced by visibility/init code, but
            // no pane builder constructs it. Skipped here so this test can act
            // as a regression guard for *new* orphans without flagging the
            // pre-existing one. Remove this skip when GameplayExtrasMore is
            // either built or deleted.
            if id == RowId::GameplayExtrasMore {
                continue;
            }
            assert!(
                built[i],
                "{id:?} is defined as a RowId but no pane builder constructs a Row for it",
            );
        }
    }

    #[test]
    fn dispatch_mirror_flag_off_keeps_per_player_index() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        // BackgroundFilter is a Numeric row with mirror_across_players: false.
        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::BackgroundFilter)
            .expect("BackgroundFilter should be in Main pane");
        state.pane_mut().selected_row[P1] = row_index;

        let row = state.pane().row_map.get(RowId::BackgroundFilter).unwrap();
        assert!(
            !row.mirror_across_players,
            "BackgroundFilter should default to per-player choice"
        );
        let p2_before = row.selected_choice_index[1];

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);

        let row = state.pane().row_map.get(RowId::BackgroundFilter).unwrap();
        assert_eq!(
            row.selected_choice_index[1], p2_before,
            "P2 slot must not move when mirror_across_players is false"
        );
    }

    #[test]
    fn dispatch_mirror_skipped_when_apply_returns_none() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        // Insert a fixture Custom row whose apply always returns Outcome::NONE,
        // even though mirror_across_players is true. The dispatcher must NOT
        // overwrite P2 in that case.
        let custom = super::CustomBinding {
            apply: |_state, _player_idx, _id, _delta, _wrap| super::Outcome::NONE,
        };
        let mirror_row = Row {
            id: RowId::Hide,
            behavior: super::RowBehavior::Custom(custom),
            name: lookup_key("PlayerOptions", "Hide"),
            choices: vec!["A".into(), "B".into(), "C".into()],
            selected_choice_index: [0, 0],
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: true,
        };
        state.pane_mut().row_map.display_order.push(RowId::Hide);
        state.pane_mut().row_map.insert(mirror_row);
        let row_index = state.pane().row_map.display_order().len() - 1;
        state.pane_mut().selected_row[P1] = row_index;

        // Pre-set P2 to a distinct value to detect any incorrect overwrite.
        state
            .pane_mut()
            .row_map
            .get_mut(RowId::Hide)
            .unwrap()
            .selected_choice_index[1] = 2;

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);

        let row = state.pane().row_map.get(RowId::Hide).unwrap();
        assert_eq!(
            row.selected_choice_index[1], 2,
            "P2 must keep its prior value when the Custom apply returns Outcome::NONE"
        );
    }

    #[test]
    fn inline_nav_what_comes_next_syncs_both_players_on_focus_commit() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let row_index = state
            .pane()
            .row_map
            .display_order()
            .iter()
            .position(|&id| id == RowId::WhatComesNext)
            .expect("WhatComesNext should be in Main pane");
        state.pane_mut().selected_row[P1] = row_index;

        let n = state
            .pane()
            .row_map
            .get(RowId::WhatComesNext)
            .unwrap()
            .choices
            .len();
        assert!(
            n >= 2,
            "WhatComesNext needs at least 2 choices for this test"
        );

        // Reset both slots to a known starting choice and target a different one.
        state
            .pane_mut()
            .row_map
            .get_mut(RowId::WhatComesNext)
            .unwrap()
            .selected_choice_index = [0, 0];
        let row = state.pane().row_map.get(RowId::WhatComesNext).unwrap();
        let left_x = super::inline_nav::inline_choice_left_x_for_row(&state, row_index);
        let centers =
            super::inline_nav::inline_choice_centers(&row.choices, &asset_manager, left_x);
        assert_eq!(centers.len(), n);
        let target = 1usize;
        state.pane_mut().inline_choice_x[P1] = centers[target];

        let changed = super::inline_nav::commit_inline_focus_selection(
            &mut state,
            &asset_manager,
            P1,
            row_index,
        );
        assert!(changed, "commit should report a change");

        let row = state.pane().row_map.get(RowId::WhatComesNext).unwrap();
        assert_eq!(
            row.selected_choice_index,
            [target, target],
            "WhatComesNext (mirror_across_players=true) must sync both player slots on inline focus commit"
        );
    }

    fn cycle_test_row(choices: &[&str], initial: [usize; 2]) -> Row {
        Row {
            id: RowId::Perspective,
            behavior: RowBehavior::Exit,
            name: lookup_key("PlayerOptions", "Perspective"),
            choices: choices.iter().map(ToString::to_string).collect(),
            selected_choice_index: initial,
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        }
    }

    fn numeric_test_row(choices: &[&str], initial: [usize; 2]) -> Row {
        Row {
            id: RowId::Spacing,
            behavior: RowBehavior::Exit,
            name: lookup_key("PlayerOptions", "Spacing"),
            choices: choices.iter().map(ToString::to_string).collect(),
            selected_choice_index: initial,
            help: Vec::new(),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        }
    }

    #[test]
    fn init_cycle_row_from_binding_uses_init_function() {
        let binding: ChoiceBinding<usize> = ChoiceBinding::<usize> {
            apply: |_, _| super::Outcome::NONE,
            persist_for_side: |_, _| {},
            init: Some(CycleInit {
                from_profile: |_| 2,
            }),
        };
        let mut row = cycle_test_row(&["A", "B", "C", "D"], [0, 0]);
        let applied = init_cycle_row_from_binding(&mut row, &binding, &Profile::default(), P1);
        assert!(applied, "binding has init; helper must apply it");
        assert_eq!(row.selected_choice_index[P1], 2);
        assert_eq!(row.selected_choice_index[P2], 0, "P2 untouched");
    }

    #[test]
    fn init_cycle_row_from_binding_clamps_to_choices_length() {
        let binding: ChoiceBinding<usize> = ChoiceBinding::<usize> {
            apply: |_, _| super::Outcome::NONE,
            persist_for_side: |_, _| {},
            init: Some(CycleInit {
                from_profile: |_| 99,
            }),
        };
        let mut row = cycle_test_row(&["A", "B", "C"], [0, 0]);
        init_cycle_row_from_binding(&mut row, &binding, &Profile::default(), P1);
        assert_eq!(
            row.selected_choice_index[P1], 2,
            "out-of-range init must clamp to choices.len()-1"
        );
    }

    #[test]
    fn init_cycle_row_from_binding_returns_false_without_init() {
        let binding: ChoiceBinding<usize> = ChoiceBinding::<usize> {
            apply: |_, _| super::Outcome::NONE,
            persist_for_side: |_, _| {},
            init: None,
        };
        let mut row = cycle_test_row(&["A", "B", "C"], [1, 1]);
        let applied = init_cycle_row_from_binding(&mut row, &binding, &Profile::default(), P1);
        assert!(!applied, "no init contract => helper reports no-op");
        assert_eq!(
            row.selected_choice_index,
            [1, 1],
            "selection must be untouched when no init is wired"
        );
    }

    #[test]
    fn init_numeric_row_from_binding_finds_matching_choice() {
        let binding = NumericBinding {
            parse: super::parse_i32_percent,
            apply: |_, _| super::Outcome::NONE,
            persist_for_side: |_, _| {},
            init: Some(NumericInit {
                from_profile: |_| 50,
                format: |v| format!("{v}%"),
            }),
        };
        let mut row = numeric_test_row(&["0%", "25%", "50%", "75%", "100%"], [0, 0]);
        let applied = init_numeric_row_from_binding(&mut row, &binding, &Profile::default(), P2);
        assert!(applied);
        assert_eq!(row.selected_choice_index[P2], 2);
        assert_eq!(row.selected_choice_index[P1], 0, "P1 untouched");
    }

    #[test]
    fn init_numeric_row_from_binding_preserves_selection_on_no_match() {
        let binding = NumericBinding {
            parse: super::parse_i32_percent,
            apply: |_, _| super::Outcome::NONE,
            persist_for_side: |_, _| {},
            init: Some(NumericInit {
                from_profile: |_| 33,
                format: |v| format!("{v}%"),
            }),
        };
        let mut row = numeric_test_row(&["0%", "50%", "100%"], [1, 1]);
        let applied = init_numeric_row_from_binding(&mut row, &binding, &Profile::default(), P1);
        assert!(
            applied,
            "binding has init; helper applied it (even if no-op)"
        );
        assert_eq!(
            row.selected_choice_index,
            [1, 1],
            "no matching choice => selection preserved"
        );
    }

    #[test]
    fn init_numeric_row_from_binding_returns_false_without_init() {
        let binding = NumericBinding {
            parse: super::parse_i32_percent,
            apply: |_, _| super::Outcome::NONE,
            persist_for_side: |_, _| {},
            init: None,
        };
        let mut row = numeric_test_row(&["0%", "50%", "100%"], [1, 1]);
        let applied = init_numeric_row_from_binding(&mut row, &binding, &Profile::default(), P1);
        assert!(!applied);
        assert_eq!(row.selected_choice_index, [1, 1]);
    }

    /// End-to-end check that the cycle/numeric init dispatchers in
    /// `apply_profile_defaults` produce the same `selected_choice_index`
    /// values that the legacy hand-written if-let blocks did, for every
    /// Main pane row migrated to the binding-driven contract.
    ///
    /// Sets a non-default value on each migrated profile field so a stale
    /// `[0, 0]` result would fail the assertion.
    #[test]
    fn apply_profile_defaults_initializes_main_pane_rows_via_contracts() {
        ensure_i18n();
        let (mut state, _asset_manager) = setup_state();

        // Mutate every profile field whose Main pane row was migrated to the
        // CycleInit / NumericInit contract.
        let p = &mut state.player_profiles[P1];
        p.perspective = Perspective::Distant;
        p.combo_font = ComboFont::Wendy;
        p.background_filter = BackgroundFilter::from_i32(42);
        p.visual_delay_ms = 35;
        p.global_offset_shift_ms = -45;

        let profile = state.player_profiles[P1].clone();
        let noteskin_names = test_noteskin_catalog().names;
        let mut main_row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Main,
            &noteskin_names,
            &[],
            &[],
            &[],
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut masks = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut main_row_map, &profile, P1, &mut masks);

        // Cycle rows: assert the selected variant matches the profile value.
        assert_variant_at_cursor(
            &main_row_map,
            RowId::Perspective,
            &super::PERSPECTIVE_VARIANTS,
            profile.perspective,
        );
        assert_variant_at_cursor(
            &main_row_map,
            RowId::ComboFont,
            &super::COMBO_FONT_VARIANTS,
            profile.combo_font,
        );

        // Numeric rows: assert the choice string at the cursor matches the
        // formatted profile value (the same lookup the dispatcher does).
        assert_choice_at_cursor(&main_row_map, RowId::BackgroundFilter, "42%");
        assert_choice_at_cursor(&main_row_map, RowId::VisualDelay, "35ms");
        assert_choice_at_cursor(&main_row_map, RowId::GlobalOffsetShift, "-45ms");
    }

    #[test]
    fn apply_profile_defaults_initializes_display_pane_rows_via_contracts() {
        ensure_i18n();
        let (mut state, _asset_manager) = setup_state();

        let p = &mut state.player_profiles[P1];
        p.perspective = Perspective::Distant;
        p.combo_font = ComboFont::Wendy;
        p.background_filter = BackgroundFilter::from_i32(42);
        p.spacing_percent = 95;
        p.judgment_offset_x = -25;
        p.judgment_offset_y = 30;
        p.combo_offset_x = 12;
        p.combo_offset_y = -8;
        p.note_field_offset_x = 17;
        p.note_field_offset_y = -22;

        let profile = state.player_profiles[P1].clone();
        let noteskin_names = test_noteskin_catalog().names;
        let mut row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Display,
            &noteskin_names,
            &[],
            &[],
            &[],
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut masks = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut row_map, &profile, P1, &mut masks);

        assert_variant_at_cursor(
            &row_map,
            RowId::Perspective,
            &super::PERSPECTIVE_VARIANTS,
            profile.perspective,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::ComboFont,
            &super::COMBO_FONT_VARIANTS,
            profile.combo_font,
        );

        assert_choice_at_cursor(&row_map, RowId::BackgroundFilter, "42%");
        assert_choice_at_cursor(&row_map, RowId::Spacing, "95%");
        assert_choice_at_cursor(&row_map, RowId::JudgmentOffsetX, "-25");
        assert_choice_at_cursor(&row_map, RowId::JudgmentOffsetY, "30");
        assert_choice_at_cursor(&row_map, RowId::ComboOffsetX, "12");
        assert_choice_at_cursor(&row_map, RowId::ComboOffsetY, "-8");
        assert_choice_at_cursor(&row_map, RowId::NoteFieldOffsetX, "17");
        assert_choice_at_cursor(&row_map, RowId::NoteFieldOffsetY, "-22");
    }

    /// Numeric values outside the row's choice range (clamped by the binding's
    /// `from_profile` closure) must still land on a valid in-range choice,
    /// matching the legacy behaviour. Picks the largest representable value
    /// for each row's clamp range; the cursor must end up on the choice that
    /// formats to the clamped value.
    #[test]
    fn apply_profile_defaults_clamps_numeric_values_to_range() {
        ensure_i18n();
        let (mut state, _asset_manager) = setup_state();

        let p = &mut state.player_profiles[P1];
        p.judgment_offset_x = 10_000; // clamps to HUD_OFFSET_MAX
        p.note_field_offset_x = -10; // clamps to 0 (range 0..50)
        p.visual_delay_ms = -10_000; // clamps to -100
        p.spacing_percent = 100_000; // clamps to SPACING_PERCENT_MAX

        let profile = state.player_profiles[P1].clone();
        let noteskin_names = test_noteskin_catalog().names;
        let mut main_row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Main,
            &noteskin_names,
            &[],
            &[],
            &[],
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut display_row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Display,
            &noteskin_names,
            &[],
            &[],
            &[],
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut masks = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut main_row_map, &profile, P1, &mut masks);
        panes::apply_profile_defaults(&mut display_row_map, &profile, P1, &mut masks);

        assert_choice_at_cursor(
            &display_row_map,
            RowId::JudgmentOffsetX,
            &HUD_OFFSET_MAX.to_string(),
        );
        assert_choice_at_cursor(&display_row_map, RowId::NoteFieldOffsetX, "0");
        assert_choice_at_cursor(&main_row_map, RowId::VisualDelay, "-100ms");
        assert_choice_at_cursor(
            &display_row_map,
            RowId::Spacing,
            &format!("{}%", super::SPACING_PERCENT_MAX),
        );
    }

    #[test]
    fn apply_profile_defaults_initializes_advanced_pane_rows_via_contracts() {
        ensure_i18n();
        let (mut state, _asset_manager) = setup_state();

        let p = &mut state.player_profiles[P1];
        p.turn_option = super::TURN_OPTION_VARIANTS[1];
        p.lifemeter_type = super::LIFE_METER_TYPE_VARIANTS[1];
        p.step_statistics = StepStatisticsMask::SONG_BANNER | StepStatisticsMask::STEP_COUNTS;
        p.score_position = super::SCORE_POSITION_VARIANTS[1];
        p.score_display_mode = super::SCORE_DISPLAY_MODE_VARIANTS[1];
        p.display_scorebox = true;
        p.target_score = super::TARGET_SCORE_VARIANTS[1];
        p.mini_indicator_score_type = super::MINI_INDICATOR_SCORE_TYPE_VARIANTS[1];
        p.mini_indicator_subtractive_display =
            super::MINI_INDICATOR_SUBTRACTIVE_DISPLAY_VARIANTS[1];
        p.mini_indicator_size = super::MINI_INDICATOR_SIZE_VARIANTS[1];
        p.mini_indicator_color = super::MINI_INDICATOR_COLOR_VARIANTS[1];
        p.mini_indicator_position = super::MINI_INDICATOR_POSITION_VARIANTS[1];
        p.combo_colors = super::COMBO_COLORS_VARIANTS[1];
        p.combo_mode = super::COMBO_MODE_VARIANTS[1];
        p.error_bar_trim = super::ERROR_BAR_TRIM_VARIANTS[1];
        p.measure_counter = super::MEASURE_COUNTER_VARIANTS[1];
        p.measure_lines = super::MEASURE_LINES_VARIANTS[1];
        p.timing_windows = super::TIMING_WINDOWS_VARIANTS[1];
        p.transparent_density_graph_bg = true;
        p.carry_combo_between_songs = true;
        p.judgment_tilt = true;
        p.judgment_back = true;
        p.error_ms_display = true;
        p.rescore_early_hits = true;
        p.custom_fantastic_window = true;
        p.error_bar_offset_x = -25;
        p.error_bar_offset_y = 30;
        p.text_error_bar_scalable = true;
        p.text_error_bar_threshold_ms = 17;
        p.center_tick = true;
        p.short_average_error_bar_enabled = false;
        p.average_error_bar_intensity = 1.5;
        p.average_error_bar_interval_ms = 700;

        let profile = state.player_profiles[P1].clone();
        let noteskin_names = test_noteskin_catalog().names;
        let mut row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Advanced,
            &noteskin_names,
            &[],
            &[],
            &[],
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut masks = PlayerOptionMasks::default();
        panes::apply_profile_defaults(&mut row_map, &profile, P1, &mut masks);

        assert_variant_at_cursor(
            &row_map,
            RowId::Turn,
            &super::TURN_OPTION_VARIANTS,
            profile.turn_option,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::LifeMeterType,
            &super::LIFE_METER_TYPE_VARIANTS,
            profile.lifemeter_type,
        );
        assert_eq!(masks.step_statistics, profile.step_statistics);
        assert_eq!(
            row_map
                .get(RowId::DataVisualizations)
                .expect("Step Statistics row present")
                .selected_choice_index[P1],
            1,
            "Step Statistics cursor should land on the first active bit",
        );
        assert!(
            masks
                .gameplay_extras
                .contains(GameplayExtrasMask::DISPLAY_SCOREBOX),
            "Gameplay Extras Display Scorebox bit set from profile",
        );
        assert!(
            masks
                .gameplay_extras_more
                .contains(GameplayExtrasMoreMask::DISPLAY_SCOREBOX),
            "derived GameplayExtrasMore Display Scorebox bit set from sibling profile field",
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::ScorePosition,
            &super::SCORE_POSITION_VARIANTS,
            profile.score_position,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::ScoreDisplay,
            &super::SCORE_DISPLAY_MODE_VARIANTS,
            profile.score_display_mode,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::TargetScore,
            &super::TARGET_SCORE_VARIANTS,
            profile.target_score,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::IndicatorScoreType,
            &super::MINI_INDICATOR_SCORE_TYPE_VARIANTS,
            profile.mini_indicator_score_type,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::MiniIndicatorSubtractiveDisplay,
            &super::MINI_INDICATOR_SUBTRACTIVE_DISPLAY_VARIANTS,
            profile.mini_indicator_subtractive_display,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::MiniIndicatorSize,
            &super::MINI_INDICATOR_SIZE_VARIANTS,
            profile.mini_indicator_size,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::MiniIndicatorColor,
            &super::MINI_INDICATOR_COLOR_VARIANTS,
            profile.mini_indicator_color,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::MiniIndicatorPosition,
            &super::MINI_INDICATOR_POSITION_VARIANTS,
            profile.mini_indicator_position,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::ComboColors,
            &super::COMBO_COLORS_VARIANTS,
            profile.combo_colors,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::ComboColorMode,
            &super::COMBO_MODE_VARIANTS,
            profile.combo_mode,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::ErrorBarTrim,
            &super::ERROR_BAR_TRIM_VARIANTS,
            profile.error_bar_trim,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::MeasureCounter,
            &super::MEASURE_COUNTER_VARIANTS,
            profile.measure_counter,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::MeasureLines,
            &super::MEASURE_LINES_VARIANTS,
            profile.measure_lines,
        );
        assert_variant_at_cursor(
            &row_map,
            RowId::TimingWindows,
            &super::TIMING_WINDOWS_VARIANTS,
            profile.timing_windows,
        );

        for id in [
            RowId::DensityGraphBackground,
            RowId::CarryCombo,
            RowId::JudgmentTilt,
            RowId::JudgmentBehindArrows,
            RowId::OffsetIndicator,
            RowId::TextErrorBarMode,
            RowId::CenterTick,
            RowId::RescoreEarlyHits,
            RowId::CustomBlueFantasticWindow,
        ] {
            let row = row_map
                .get(id)
                .unwrap_or_else(|| panic!("Row {id:?} missing"));
            assert_eq!(
                row.selected_choice_index[P1], 1,
                "bool row {id:?} should be at index 1 (true)"
            );
        }

        assert_choice_at_cursor(&row_map, RowId::ErrorBarOffsetX, "-25");
        assert_choice_at_cursor(&row_map, RowId::ErrorBarOffsetY, "30");
        assert_choice_at_cursor(&row_map, RowId::TextErrorBarMode, "Scalable");
        assert_choice_at_cursor(&row_map, RowId::TextErrorBarThreshold, "17ms");
        assert_choice_at_cursor(&row_map, RowId::CenterTick, "On");
        assert_choice_at_cursor(&row_map, RowId::ShortAverageErrorBar, "Off");
        assert_choice_at_cursor(&row_map, RowId::AverageErrorBarIntensity, "1.50x");
        assert_choice_at_cursor(&row_map, RowId::AverageErrorBarInterval, "700ms");
    }

    fn assert_choice_at_cursor(row_map: &RowMap, id: RowId, expected: &str) {
        let row = row_map
            .get(id)
            .unwrap_or_else(|| panic!("Row {id:?} missing from Main pane row map"));
        let idx = row.selected_choice_index[P1];
        let actual = row.choices.get(idx).map(String::as_str).unwrap_or("<oob>");
        assert_eq!(
            actual, expected,
            "Row {id:?}: cursor at {idx} points to {actual:?}, expected {expected:?}"
        );
    }

    fn assert_variant_at_cursor<T: Copy + PartialEq + std::fmt::Debug>(
        row_map: &RowMap,
        id: RowId,
        variants: &[T],
        expected: T,
    ) {
        let row = row_map
            .get(id)
            .unwrap_or_else(|| panic!("Row {id:?} missing from Main pane row map"));
        let idx = row.selected_choice_index[P1];
        let actual = variants
            .get(idx)
            .copied()
            .unwrap_or_else(|| panic!("Row {id:?}: cursor {idx} out of variant range"));
        assert_eq!(
            actual, expected,
            "Row {id:?}: variant at cursor {idx} = {actual:?}, expected {expected:?}"
        );
    }

    use deadsync_profile::{
        AccelEffectsMask, AppearanceEffectsMask, HoldsMask, InsertMask, RemoveMask,
        VisualEffectsMask,
    };

    fn install_bitmask_row(
        state: &mut super::State,
        id: RowId,
        binding: BitmaskBinding,
        choices: &[&str],
        choice_index: usize,
    ) -> usize {
        let row = test_bitmask_row(id, lookup_key("PlayerOptions", "Insert"), choices, binding);
        state.pane_mut().row_map.display_order.push(id);
        state.pane_mut().row_map.insert(row);
        let row_index = state.pane().row_map.display_order().len() - 1;
        state
            .pane_mut()
            .row_map
            .get_mut(id)
            .unwrap()
            .selected_choice_index = [choice_index, choice_index];
        state.pane_mut().selected_row[P1] = row_index;
        row_index
    }

    #[test]
    fn generic_toggle_insert_row_sets_bit_and_profile() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| p.insert_active_mask.bits() as u32,
                get_active: |m| m.insert.bits() as u32,
                set_active: |m, b| m.insert = InsertMask::from_bits_retain(b as u8),
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, p, b| {
                    p.insert_active_mask = InsertMask::from_bits_truncate(b as u8);
                },
                persist_for_side: |s, p| {
                    profile::update_insert_mask_for_side(s, p.insert_active_mask);
                },
                bit_mapping: BitMapping::Sequential { width: 7 },
                sync_visibility: false,
            },
        };
        install_bitmask_row(
            &mut state,
            RowId::Insert,
            binding,
            &["W", "B", "Q", "M", "S", "E", "T"],
            2,
        );
        state.option_masks[P1].insert = InsertMask::empty();
        state.player_profiles[P1].insert_active_mask = InsertMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_eq!(
            state.option_masks[P1].insert.bits(),
            1u8 << 2,
            "Insert bit at choice index 2 should be set"
        );
        assert_eq!(
            state.player_profiles[P1].insert_active_mask.bits(),
            1u8 << 2,
            "Insert profile should mirror the mask"
        );

        // Toggle again to clear.
        handle_start_event(&mut state, &asset_manager, active, P1);
        assert_eq!(state.option_masks[P1].insert, InsertMask::empty());
        assert_eq!(
            state.player_profiles[P1].insert_active_mask,
            InsertMask::empty()
        );
    }

    #[test]
    fn generic_toggle_insert_row_ignores_out_of_width_choice() {
        // Insert clamps to choice_index < 7. A row with 7 choices and a
        // selected index of 7 (impossible in practice; defensive) must
        // produce no toggle.
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        let binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| p.insert_active_mask.bits() as u32,
                get_active: |m| m.insert.bits() as u32,
                set_active: |m, b| m.insert = InsertMask::from_bits_retain(b as u8),
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, p, b| {
                    p.insert_active_mask = InsertMask::from_bits_truncate(b as u8);
                },
                persist_for_side: |s, p| {
                    profile::update_insert_mask_for_side(s, p.insert_active_mask);
                },
                bit_mapping: BitMapping::Sequential { width: 7 },
                sync_visibility: false,
            },
        };
        // 8 choices, cursor at index 7 — out of width.
        install_bitmask_row(
            &mut state,
            RowId::Insert,
            binding,
            &["a", "b", "c", "d", "e", "f", "g", "h"],
            7,
        );
        state.option_masks[P1].insert = InsertMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_eq!(
            state.option_masks[P1].insert,
            InsertMask::empty(),
            "out-of-width choice index must not toggle a bit"
        );
    }

    #[test]
    fn generic_toggle_remove_row_sets_bit_and_profile() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        let binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| p.remove_active_mask.bits() as u32,
                get_active: |m| m.remove.bits() as u32,
                set_active: |m, b| m.remove = RemoveMask::from_bits_retain(b as u8),
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, p, b| {
                    p.remove_active_mask = RemoveMask::from_bits_truncate(b as u8);
                },
                persist_for_side: |s, p| {
                    profile::update_remove_mask_for_side(s, p.remove_active_mask);
                },
                bit_mapping: BitMapping::Sequential { width: 8 },
                sync_visibility: false,
            },
        };
        install_bitmask_row(
            &mut state,
            RowId::Remove,
            binding,
            &["L", "M", "H", "J", "Hands", "Q", "Lifts", "Fakes"],
            5,
        );
        state.option_masks[P1].remove = RemoveMask::empty();
        state.player_profiles[P1].remove_active_mask = RemoveMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_eq!(state.option_masks[P1].remove.bits(), 1u8 << 5);
        assert_eq!(
            state.player_profiles[P1].remove_active_mask.bits(),
            1u8 << 5
        );
    }

    #[test]
    fn generic_toggle_holds_row_sets_bit_and_profile() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        let binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| p.holds_active_mask.bits() as u32,
                get_active: |m| m.holds.bits() as u32,
                set_active: |m, b| m.holds = HoldsMask::from_bits_retain(b as u8),
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, p, b| {
                    p.holds_active_mask = HoldsMask::from_bits_truncate(b as u8);
                },
                persist_for_side: |s, p| {
                    profile::update_holds_mask_for_side(s, p.holds_active_mask);
                },
                bit_mapping: BitMapping::Sequential { width: 5 },
                sync_visibility: false,
            },
        };
        // Holds in production has 5 choices; bit_mapping is Sequential { width: 5 }.
        install_bitmask_row(
            &mut state,
            RowId::Holds,
            binding,
            &["Planted", "Floored", "Twister", "NoRolls", "ToRolls"],
            3,
        );
        state.option_masks[P1].holds = HoldsMask::empty();
        state.player_profiles[P1].holds_active_mask = HoldsMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_eq!(state.option_masks[P1].holds.bits(), 1u8 << 3);
        assert_eq!(state.player_profiles[P1].holds_active_mask.bits(), 1u8 << 3);
    }

    #[test]
    fn generic_toggle_accel_row_sets_bit_and_profile() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        let binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| p.accel_effects_active_mask.bits() as u32,
                get_active: |m| m.accel_effects.bits() as u32,
                set_active: |m, b| m.accel_effects = AccelEffectsMask::from_bits_retain(b as u8),
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, p, b| {
                    p.accel_effects_active_mask = AccelEffectsMask::from_bits_truncate(b as u8);
                },
                persist_for_side: |s, p| {
                    profile::update_accel_effects_mask_for_side(s, p.accel_effects_active_mask);
                },
                bit_mapping: BitMapping::Sequential { width: 5 },
                sync_visibility: false,
            },
        };
        install_bitmask_row(
            &mut state,
            RowId::Accel,
            binding,
            &["Boost", "Brake", "Wave", "Expand", "Boomerang"],
            1,
        );
        state.option_masks[P1].accel_effects = AccelEffectsMask::empty();
        state.player_profiles[P1].accel_effects_active_mask = AccelEffectsMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_eq!(state.option_masks[P1].accel_effects.bits(), 1u8 << 1);
        assert_eq!(
            state.player_profiles[P1].accel_effects_active_mask.bits(),
            1u8 << 1
        );
    }

    #[test]
    fn generic_toggle_visual_effects_row_sets_bit_and_profile() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        let binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| p.visual_effects_active_mask.bits() as u32,
                get_active: |m| m.visual_effects.bits() as u32,
                set_active: |m, b| m.visual_effects = VisualEffectsMask::from_bits_retain(b as u16),
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, p, b| {
                    p.visual_effects_active_mask = VisualEffectsMask::from_bits_truncate(b as u16);
                },
                persist_for_side: |s, p| {
                    profile::update_visual_effects_mask_for_side(s, p.visual_effects_active_mask);
                },
                bit_mapping: BitMapping::Sequential { width: 10 },
                sync_visibility: false,
            },
        };
        install_bitmask_row(
            &mut state,
            RowId::Effect,
            binding,
            &[
                "Drunk",
                "Dizzy",
                "Confusion",
                "Big",
                "Flip",
                "Invert",
                "Tornado",
                "Tipsy",
                "Bumpy",
                "Beat",
            ],
            9,
        );
        state.option_masks[P1].visual_effects = VisualEffectsMask::empty();
        state.player_profiles[P1].visual_effects_active_mask = VisualEffectsMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_eq!(state.option_masks[P1].visual_effects.bits(), 1u16 << 9);
        assert_eq!(
            state.player_profiles[P1].visual_effects_active_mask.bits(),
            1u16 << 9
        );
    }

    #[test]
    fn generic_toggle_appearance_row_sets_bit_and_profile() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();
        let binding = BitmaskBinding::Generic {
            init: BitmaskInit {
                from_profile: |p| p.appearance_effects_active_mask.bits() as u32,
                get_active: |m| m.appearance_effects.bits() as u32,
                set_active: |m, b| {
                    m.appearance_effects = AppearanceEffectsMask::from_bits_retain(b as u8)
                },
                cursor: CursorInit::FirstActiveBit,
            },
            writeback: BitmaskWriteback {
                project: |_, p, b| {
                    p.appearance_effects_active_mask =
                        AppearanceEffectsMask::from_bits_truncate(b as u8);
                },
                persist_for_side: |s, p| {
                    profile::update_appearance_effects_mask_for_side(
                        s,
                        p.appearance_effects_active_mask,
                    );
                },
                bit_mapping: BitMapping::Sequential { width: 5 },
                sync_visibility: false,
            },
        };
        install_bitmask_row(
            &mut state,
            RowId::Appearance,
            binding,
            &["Hidden", "Sudden", "Stealth", "Blink", "RVanish"],
            4,
        );
        state.option_masks[P1].appearance_effects = AppearanceEffectsMask::empty();
        state.player_profiles[P1].appearance_effects_active_mask = AppearanceEffectsMask::empty();

        let active = session_active_players();
        handle_start_event(&mut state, &asset_manager, active, P1);

        assert_eq!(state.option_masks[P1].appearance_effects.bits(), 1u8 << 4);
        assert_eq!(
            state.player_profiles[P1]
                .appearance_effects_active_mask
                .bits(),
            1u8 << 4
        );
    }
}
