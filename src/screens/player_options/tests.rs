use super::*;

#[cfg(test)]
pub(super) mod tests {
    use super::{
        BitmaskBinding, BitmaskInit, ChoiceBinding, CursorInit, CycleInit, ErrorBarMask,
        FaPlusMask, GameplayExtrasMask, GameplayExtrasMoreMask, HUD_OFFSET_MAX, HUD_OFFSET_MIN,
        HUD_OFFSET_ZERO_INDEX, HideMask, NAV_INITIAL_HOLD_DELAY, NAV_REPEAT_SCROLL_INTERVAL,
        NumericBinding, NumericInit, P1, P2, PlayerOptionMasks, Row, RowBehavior, RowId, RowMap,
        ScrollMask, SpeedMod, SpeedModType, handle_arcade_start_event, handle_start_event,
        hud_offset_choices, init_cycle_row_from_binding, init_numeric_row_from_binding,
        is_row_visible, judgment_tilt_intensity_visible, repeat_held_arcade_start, row_visibility,
        session_active_players, sync_profile_scroll_speed,
    };
    use crate::assets::AssetManager;
    use crate::assets::i18n::{LookupKey, lookup_key};
    use crate::game::profile::{self, BackgroundFilter, PlayStyle, PlayerSide, Profile};
    use crate::game::scroll::ScrollSpeedSetting;
    use crate::screens::{Screen, ScreenAction};
    use crate::test_support::{compose_scenarios, notefield_bench};
    use std::time::{Duration, Instant};

    fn ensure_i18n() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            crate::assets::i18n::init("en");
        });
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

    #[test]
    fn target_score_hides_until_score_dependent_option_is_active() {
        ensure_i18n();
        let mut row_map = test_row_map(vec![
            test_row(
                RowId::DataVisualizations,
                lookup_key("PlayerOptions", "DataVisualizations"),
                &["None", "Target Score Graph", "Step Statistics"],
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
            .get_mut(RowId::DataVisualizations)
            .unwrap()
            .selected_choice_index[P1] = 1;
        assert!(target_score_visible_for(&row_map));

        row_map
            .get_mut(RowId::DataVisualizations)
            .unwrap()
            .selected_choice_index[P1] = 0;
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
        profile.scroll_option = profile::ScrollOption::Reverse.union(profile::ScrollOption::Cross);

        let mut main_rows = test_row_map(vec![test_row(
            RowId::Exit,
            lookup_key("PlayerOptions", "Exit"),
            &["Exit"],
            [0, 0],
        )]);
        let scroll_binding = BitmaskBinding {
            toggle: super::super::choice::toggle_scroll_row,
            init: Some(BitmaskInit {
                from_profile: |p| {
                    use crate::game::profile::ScrollOption;
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
            }),
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
        super::super::panes::apply_profile_defaults(&mut main_rows, &profile, P1, &mut main);
        // Main alone: Scroll row absent, mask comes back empty (the bug source).
        assert_eq!(main.scroll, ScrollMask::empty());

        // Accumulated across all three panes (the fix): Reverse + Cross preserved.
        let mut combined = PlayerOptionMasks::default();
        super::super::panes::apply_profile_defaults(&mut main_rows, &profile, P1, &mut combined);
        super::super::panes::apply_profile_defaults(
            &mut advanced_rows,
            &profile,
            P1,
            &mut combined,
        );
        super::super::panes::apply_profile_defaults(
            &mut uncommon_rows,
            &profile,
            P1,
            &mut combined,
        );
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

        let hide_binding = BitmaskBinding {
            toggle: super::super::choice::toggle_hide_row,
            init: Some(BitmaskInit {
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
            }),
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
        super::super::panes::apply_profile_defaults(&mut hide_rows, &profile, P1, &mut masks);

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

        let fa_plus_binding = BitmaskBinding {
            toggle: super::super::choice::toggle_fa_plus_row,
            init: Some(BitmaskInit {
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
            }),
        };
        let mut fa_plus_rows = test_row_map(vec![test_bitmask_row(
            RowId::FAPlusOptions,
            lookup_key("PlayerOptions", "FAPlusOptions"),
            &["Window", "EX", "HardEX", "Pane", "Blue10", "Split"],
            fa_plus_binding,
        )]);

        let mut masks = PlayerOptionMasks::default();
        super::super::panes::apply_profile_defaults(&mut fa_plus_rows, &profile, P1, &mut masks);

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
    /// `GameplayExtras` profile processing (`column_cues` and
    /// `display_scorebox` toggles contribute to BOTH masks). A row-driven
    /// mask registry must explicitly handle this derivation.
    #[test]
    fn init_gameplay_extras_more_derived_from_sibling_profile_fields() {
        ensure_i18n();
        let mut profile = Profile::default();
        profile.column_cues = true;
        profile.display_scorebox = true;

        // No GameplayExtrasMore row exists (orphan; see the
        // `every_row_id_is_constructed_by_some_pane` test) — we still expect
        // the derived mask bits to be populated.
        let gameplay_extras_binding = BitmaskBinding {
            toggle: super::super::choice::toggle_gameplay_extras_row,
            init: Some(BitmaskInit {
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
                    if p.display_scorebox {
                        bits.insert(GameplayExtrasMask::DISPLAY_SCOREBOX);
                    }
                    bits.bits() as u32
                },
                get_active: |m| m.gameplay_extras.bits() as u32,
                set_active: |m, b| {
                    m.gameplay_extras = GameplayExtrasMask::from_bits_retain(b as u8);
                },
                cursor: CursorInit::FirstActiveBit,
            }),
        };
        let mut rows = test_row_map(vec![test_bitmask_row(
            RowId::GameplayExtras,
            lookup_key("PlayerOptions", "GameplayExtras"),
            &["FlashMiss", "DensityTop", "ColumnCues", "Scorebox"],
            gameplay_extras_binding,
        )]);

        let mut masks = PlayerOptionMasks::default();
        super::super::panes::apply_profile_defaults(&mut rows, &profile, P1, &mut masks);

        assert!(
            masks
                .gameplay_extras
                .contains(GameplayExtrasMask::COLUMN_CUES),
            "GameplayExtras COLUMN_CUES bit set from profile",
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
        let base = notefield_bench::fixture();
        let song = base.state().song.clone();

        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);

        let mut asset_manager = AssetManager::new();
        for (name, font) in compose_scenarios::bench_fonts() {
            asset_manager.register_font(name, font);
        }

        let mut state = super::init(song, [0; 2], [0; 2], 1, Screen::SelectMusic, None);
        let active = session_active_players();
        let first_row = state.pane().selected_row[P1];
        assert!(handle_arcade_start_event(&mut state, &asset_manager, active, P1).is_none());
        let second_row = state.pane().selected_row[P1];
        assert!(second_row > first_row);

        let now = Instant::now();
        state.start_input[P1].held_since =
            Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
        state.start_input[P1].last_triggered_at =
            Some(now - NAV_REPEAT_SCROLL_INTERVAL - Duration::from_millis(1));

        assert!(repeat_held_arcade_start(&mut state, &asset_manager, active, P1, now).is_none());
        assert!(state.pane().selected_row[P1] > second_row);
    }

    #[test]
    fn held_arcade_start_stops_at_exit_row() {
        ensure_i18n();
        let base = notefield_bench::fixture();
        let song = base.state().song.clone();

        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);

        let mut asset_manager = AssetManager::new();
        for (name, font) in compose_scenarios::bench_fonts() {
            asset_manager.register_font(name, font);
        }

        let mut state = super::init(song, [0; 2], [0; 2], 1, Screen::SelectMusic, None);
        let active = session_active_players();
        let last_row = state.pane().row_map.len().saturating_sub(1);
        state.pane_mut().selected_row[P1] = last_row;
        state.pane_mut().prev_selected_row[P1] = last_row;

        let now = Instant::now();
        state.start_input[P1].held_since =
            Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
        state.start_input[P1].last_triggered_at =
            Some(now - NAV_REPEAT_SCROLL_INTERVAL - Duration::from_millis(1));

        assert!(repeat_held_arcade_start(&mut state, &asset_manager, active, P1, now).is_none());
        assert_eq!(state.pane().selected_row[P1], last_row);
    }

    fn setup_state() -> (super::State, AssetManager) {
        let base = notefield_bench::fixture();
        let song = base.state().song.clone();
        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);
        let mut asset_manager = AssetManager::new();
        for (name, font) in compose_scenarios::bench_fonts() {
            asset_manager.register_font(name, font);
        }
        let state = super::init(song, [0; 2], [0; 2], 1, Screen::SelectMusic, None);
        (state, asset_manager)
    }

    fn setup_versus_state() -> (super::State, AssetManager) {
        let base = notefield_bench::fixture();
        let song = base.state().song.clone();
        profile::set_session_play_style(PlayStyle::Versus);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, true);
        let mut asset_manager = AssetManager::new();
        for (name, font) in compose_scenarios::bench_fonts() {
            asset_manager.register_font(name, font);
        }
        let state = super::init(song, [0; 2], [0; 2], 1, Screen::SelectMusic, None);
        (state, asset_manager)
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

        // Insert a Scroll row directly since it lives in the Advanced pane.
        let scroll_row = Row {
            id: RowId::Scroll,
            behavior: super::RowBehavior::Bitmask(super::BitmaskBinding {
                toggle: super::choice::toggle_scroll_row,
                init: None,
            }),
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
            !judgment_tilt_intensity_visible(&state.pane().row_map, active),
            "JudgmentTiltIntensity should start hidden"
        );

        // Advance to index 1 (enabled) — apply returns persisted_with_visibility → syncs
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1, super::NavWrap::Wrap);

        assert!(
            judgment_tilt_intensity_visible(&state.pane().row_map, active),
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
            matches!(action, Some(ScreenAction::Navigate(Screen::Gameplay))),
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

        let active = session_active_players();
        let action = handle_start_event(&mut state, &asset_manager, active, P1);
        assert!(
            matches!(action, Some(ScreenAction::Navigate(Screen::Practice))),
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

        let active = session_active_players();
        let action = handle_start_event(&mut state, &asset_manager, active, P1);
        assert!(
            matches!(action, Some(ScreenAction::Navigate(Screen::SelectMusic))),
            "choose different song from practice options should return to the wheel"
        );
    }

    #[test]
    fn dispatch_on_bitmask_via_delta_is_no_op() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        // Insert a Bitmask row (Scroll lives in the Advanced pane, so attach it
        // to the Main row_map directly for this isolated test).
        let scroll_row = Row {
            id: RowId::Scroll,
            behavior: super::RowBehavior::Bitmask(super::BitmaskBinding {
                toggle: super::choice::toggle_scroll_row,
                init: None,
            }),
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
        let noteskin_names = super::discover_noteskin_names();
        [
            super::OptionsPane::Main,
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
                Screen::SelectMusic,
                state.fixed_stepchart.as_ref(),
            );
            (pane, map)
        })
        .collect()
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
        p.perspective = profile::Perspective::Distant;
        p.combo_font = profile::ComboFont::Wendy;
        p.background_filter = profile::BackgroundFilter::from_i32(42);
        p.spacing_percent = 95;
        p.judgment_offset_x = -25;
        p.judgment_offset_y = 30;
        p.combo_offset_x = 12;
        p.combo_offset_y = -8;
        p.note_field_offset_x = 17;
        p.note_field_offset_y = -22;
        p.visual_delay_ms = 35;
        p.global_offset_shift_ms = -45;

        let profile = state.player_profiles[P1].clone();
        let noteskin_names = super::discover_noteskin_names();
        let mut row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Main,
            &noteskin_names,
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut masks = PlayerOptionMasks::default();
        super::super::panes::apply_profile_defaults(&mut row_map, &profile, P1, &mut masks);

        // Cycle rows: assert the selected variant matches the profile value.
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

        // Numeric rows: assert the choice string at the cursor matches the
        // formatted profile value (the same lookup the dispatcher does).
        assert_choice_at_cursor(&row_map, RowId::BackgroundFilter, "42%");
        assert_choice_at_cursor(&row_map, RowId::Spacing, "95%");
        assert_choice_at_cursor(&row_map, RowId::JudgmentOffsetX, "-25");
        assert_choice_at_cursor(&row_map, RowId::JudgmentOffsetY, "30");
        assert_choice_at_cursor(&row_map, RowId::ComboOffsetX, "12");
        assert_choice_at_cursor(&row_map, RowId::ComboOffsetY, "-8");
        assert_choice_at_cursor(&row_map, RowId::NoteFieldOffsetX, "17");
        assert_choice_at_cursor(&row_map, RowId::NoteFieldOffsetY, "-22");
        assert_choice_at_cursor(&row_map, RowId::VisualDelay, "35ms");
        assert_choice_at_cursor(&row_map, RowId::GlobalOffsetShift, "-45ms");
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
        let noteskin_names = super::discover_noteskin_names();
        let mut row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Main,
            &noteskin_names,
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut masks = PlayerOptionMasks::default();
        super::super::panes::apply_profile_defaults(&mut row_map, &profile, P1, &mut masks);

        assert_choice_at_cursor(
            &row_map,
            RowId::JudgmentOffsetX,
            &HUD_OFFSET_MAX.to_string(),
        );
        assert_choice_at_cursor(&row_map, RowId::NoteFieldOffsetX, "0");
        assert_choice_at_cursor(&row_map, RowId::VisualDelay, "-100ms");
        assert_choice_at_cursor(
            &row_map,
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
        p.data_visualizations = super::DATA_VISUALIZATIONS_VARIANTS[1];
        p.target_score = super::TARGET_SCORE_VARIANTS[1];
        p.mini_indicator_score_type = super::MINI_INDICATOR_SCORE_TYPE_VARIANTS[1];
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

        let profile = state.player_profiles[P1].clone();
        let noteskin_names = super::discover_noteskin_names();
        let mut row_map = super::build_rows(
            &state.song,
            &state.speed_mod[P1],
            state.chart_steps_index,
            [0; 2],
            state.music_rate,
            super::OptionsPane::Advanced,
            &noteskin_names,
            Screen::SelectMusic,
            state.fixed_stepchart.as_ref(),
        );
        let mut masks = PlayerOptionMasks::default();
        super::super::panes::apply_profile_defaults(&mut row_map, &profile, P1, &mut masks);

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
        assert_variant_at_cursor(
            &row_map,
            RowId::DataVisualizations,
            &super::DATA_VISUALIZATIONS_VARIANTS,
            profile.data_visualizations,
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
}
