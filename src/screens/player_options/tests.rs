use super::*;

#[cfg(test)]
pub(super) mod tests {
    use super::{
        HUD_OFFSET_MAX, HUD_OFFSET_MIN, HUD_OFFSET_ZERO_INDEX, NAV_INITIAL_HOLD_DELAY,
        NAV_REPEAT_SCROLL_INTERVAL, P1, Row, RowId, RowMap, SpeedMod,
        handle_arcade_start_event, hud_offset_choices, is_row_visible, repeat_held_arcade_start,
        row_visibility, session_active_players, sync_profile_scroll_speed,
    };
    use crate::assets::AssetManager;
    use crate::assets::i18n::{LookupKey, lookup_key};
    use crate::game::profile::{self, PlayStyle, PlayerSide, Profile};
    use crate::game::scroll::ScrollSpeedSetting;
    use crate::screens::Screen;
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
            name,
            choices: choices.iter().map(ToString::to_string).collect(),
            selected_choice_index,
            help: Vec::new(),
            choice_difficulty_indices: None,
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
                mod_type: "X".to_string(),
                value: 1.5,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::XMod(1.5));

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: "M".to_string(),
                value: 750.0,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::MMod(750.0));

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: "C".to_string(),
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
        let visibility = row_visibility(&row_map, [true, false], [0, 0], [0, 0], false);
        assert!(!is_row_visible(&row_map, 1, visibility));

        let visibility = row_visibility(&row_map, [true, false], [0, 0], [1, 0], false);
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
        let visibility = row_visibility(&row_map, [true, false], [0, 0], [0, 0], false);
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
        let visibility = row_visibility(&row_map, [true, false], [0, 0], [0, 0], false);
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
        let visibility = row_visibility(&row_map, [true, true], [0, 0], [0, 0], false);
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
        let visibility = row_visibility(&row_map, [true, true], [0, 0], [0, 0], false);
        assert!(is_row_visible(&row_map, 1, visibility));
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
        let first_row = state.selected_row[P1];
        assert!(handle_arcade_start_event(&mut state, &asset_manager, active, P1).is_none());
        let second_row = state.selected_row[P1];
        assert!(second_row > first_row);

        let now = Instant::now();
        state.start_held_since[P1] = Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
        state.start_last_triggered_at[P1] =
            Some(now - NAV_REPEAT_SCROLL_INTERVAL - Duration::from_millis(1));

        assert!(repeat_held_arcade_start(&mut state, &asset_manager, active, P1, now).is_none());
        assert!(state.selected_row[P1] > second_row);
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
        let last_row = state.row_map.len().saturating_sub(1);
        state.selected_row[P1] = last_row;
        state.prev_selected_row[P1] = last_row;

        let now = Instant::now();
        state.start_held_since[P1] = Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
        state.start_last_triggered_at[P1] =
            Some(now - NAV_REPEAT_SCROLL_INTERVAL - Duration::from_millis(1));

        assert!(repeat_held_arcade_start(&mut state, &asset_manager, active, P1, now).is_none());
        assert_eq!(state.selected_row[P1], last_row);
    }
}
