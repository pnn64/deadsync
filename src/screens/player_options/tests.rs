use super::*;

#[cfg(test)]
pub(super) mod tests {
    use super::{
        ErrorBarMask, HUD_OFFSET_MAX, HUD_OFFSET_MIN, HUD_OFFSET_ZERO_INDEX, HideMask,
        NAV_INITIAL_HOLD_DELAY, NAV_REPEAT_SCROLL_INTERVAL, P1, P2, PlayerOptionMasks, Row, RowId,
        RowMap, ScrollMask, SpeedMod, SpeedModType, handle_arcade_start_event, handle_start_event,
        hud_offset_choices, is_row_visible, judgment_tilt_intensity_visible,
        repeat_held_arcade_start, row_visibility, session_active_players, sync_profile_scroll_speed,
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
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
            ],
            false,
        );
        assert!(!is_row_visible(&row_map, 1, visibility));

        let visibility = row_visibility(
            &row_map,
            [true, false],
            [
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::COLORFUL, ..Default::default() },
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
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
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
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
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
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
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
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
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
                PlayerOptionMasks { hide: HideMask::empty(), error_bar: ErrorBarMask::empty(), ..Default::default() },
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
        profile.scroll_option = profile::ScrollOption::Reverse.union(profile::ScrollOption::Cross);

        let mut main_rows = test_row_map(vec![test_row(
            RowId::Exit,
            lookup_key("PlayerOptions", "Exit"),
            &["Exit"],
            [0, 0],
        )]);
        let mut advanced_rows = test_row_map(vec![test_row(
            RowId::Scroll,
            lookup_key("PlayerOptions", "Scroll"),
            &["Reverse", "Split", "Alternate", "Cross", "Centered"],
            [0, 0],
        )]);
        let mut uncommon_rows = test_row_map(vec![test_row(
            RowId::Exit,
            lookup_key("PlayerOptions", "Exit"),
            &["Exit"],
            [0, 0],
        )]);

        let main = super::super::panes::apply_profile_defaults(&mut main_rows, &profile, P1);
        let adv = super::super::panes::apply_profile_defaults(&mut advanced_rows, &profile, P1);
        let unc = super::super::panes::apply_profile_defaults(&mut uncommon_rows, &profile, P1);

        // Main alone: Scroll row absent, mask comes back empty (the bug source).
        assert_eq!(main.scroll, ScrollMask::empty());
        // Accumulated across all three panes (the fix): Reverse + Cross preserved.
        let combined = main.merge(adv).merge(unc);
        assert!(
            combined.scroll.contains(ScrollMask::REVERSE),
            "Reverse bit preserved after OR-accumulation"
        );
        assert!(
            combined.scroll.contains(ScrollMask::CROSS),
            "Cross bit preserved after OR-accumulation"
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
        state.start_input[P1].held_since = Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
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
        state.start_input[P1].held_since = Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
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
        state.player_profiles[P1].background_filter = BackgroundFilter::Darkest;
        state.pane_mut().selected_row[P1] = row_index;

        // delta=0 should still apply the current choice
        super::change_choice_for_player(&mut state, &asset_manager, P1, 0);

        assert_eq!(
            state.player_profiles[P1].background_filter,
            BackgroundFilter::Off,
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

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);

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
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);

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

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);

        let row = state.pane().row_map.get(RowId::BackgroundFilter).unwrap();
        assert_eq!(row.selected_choice_index[0], 1, "P1 should have advanced");
        assert_eq!(
            row.selected_choice_index[1], 0,
            "non-mirrored Cycle::Index must not touch P2's slot"
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
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);
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
        super::change_choice_for_player(&mut state, &asset_manager, P1, -1);
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
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);
        super::change_choice_for_player(&mut state, &asset_manager, P1, -3);

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
    fn dispatch_on_bitmask_via_delta_is_no_op() {
        ensure_i18n();
        let (mut state, asset_manager) = setup_state();

        // Insert a Bitmask row (Scroll lives in the Advanced pane, so attach it
        // to the Main row_map directly for this isolated test).
        let scroll_row = Row {
            id: RowId::Scroll,
            behavior: super::RowBehavior::Bitmask(super::BitmaskBinding {
                toggle: super::choice::toggle_scroll_row,
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
        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);
        super::change_choice_for_player(&mut state, &asset_manager, P1, -1);

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

        // BackgroundFilter is a Cycle row with mirror_across_players: false.
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

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);

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
            apply: |_state, _player_idx, _id, _delta| super::Outcome::NONE,
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

        super::change_choice_for_player(&mut state, &asset_manager, P1, 1);

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
}
