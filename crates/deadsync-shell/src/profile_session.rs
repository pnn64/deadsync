use std::path::Path;

use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::compat as profile;
use deadsync_profile::{
    PlayMode, PlayStyle, PlayerSide, TimingTickMode, play_style_for_joined, player_side_index,
};

use crate::SessionState;

const OPERATOR_RESET_STYLE: PlayStyle = PlayStyle::Single;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComboCarryUpdate {
    pub side: PlayerSide,
    pub combo: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayComboCarryContext {
    pub autoplay_used: bool,
    pub play_style: PlayStyle,
    pub active_side: PlayerSide,
    pub player_combos: [Option<u32>; MAX_PLAYERS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProfileSelectionSessionPlan {
    pub active_side: PlayerSide,
    pub p1_joined: bool,
    pub p2_joined: bool,
    pub play_style: PlayStyle,
}

pub(crate) const fn profile_selection_session_plan(
    current_style: PlayStyle,
    p1_joined: bool,
    p2_joined: bool,
) -> ProfileSelectionSessionPlan {
    ProfileSelectionSessionPlan {
        active_side: if p1_joined {
            PlayerSide::P1
        } else {
            PlayerSide::P2
        },
        p1_joined,
        p2_joined,
        play_style: play_style_for_joined(current_style, p1_joined, p2_joined),
    }
}

pub const fn gameplay_combo_carry_updates(
    context: GameplayComboCarryContext,
) -> [Option<ComboCarryUpdate>; MAX_PLAYERS] {
    if context.autoplay_used {
        return [None; MAX_PLAYERS];
    }
    match context.play_style {
        PlayStyle::Versus => [
            match context.player_combos[0] {
                Some(combo) => Some(ComboCarryUpdate {
                    side: PlayerSide::P1,
                    combo,
                }),
                None => None,
            },
            match context.player_combos[1] {
                Some(combo) => Some(ComboCarryUpdate {
                    side: PlayerSide::P2,
                    combo,
                }),
                None => None,
            },
        ],
        PlayStyle::Single | PlayStyle::Double => [
            match context.player_combos[0] {
                Some(combo) => Some(ComboCarryUpdate {
                    side: context.active_side,
                    combo,
                }),
                None => None,
            },
            None,
        ],
    }
}

pub fn persist_gameplay_combo_carry<EvaluationPage>(
    session: &mut SessionState<EvaluationPage>,
    autoplay_used: bool,
    player_combos: [Option<u32>; MAX_PLAYERS],
) {
    let updates = gameplay_combo_carry_updates(GameplayComboCarryContext {
        autoplay_used,
        play_style: profile::get_session_play_style(),
        active_side: profile::get_session_player_side(),
        player_combos,
    });
    for update in updates.into_iter().flatten() {
        session.combo_carry[player_side_index(update.side)] = update.combo;
        profile::update_current_combo_for_side(update.side, update.combo);
    }
}

pub const fn course_last_played_sides(
    play_style: PlayStyle,
    active_side: PlayerSide,
) -> [Option<PlayerSide>; MAX_PLAYERS] {
    match play_style {
        PlayStyle::Versus => [Some(PlayerSide::P1), Some(PlayerSide::P2)],
        PlayStyle::Single | PlayStyle::Double => [Some(active_side), None],
    }
}

pub fn record_last_played_course(course_path: &Path, difficulty_name: &str) {
    let play_style = profile::get_session_play_style();
    for side in course_last_played_sides(play_style, profile::get_session_player_side())
        .into_iter()
        .flatten()
    {
        profile::update_last_played_course_for_side(
            side,
            play_style,
            course_path,
            Some(difficulty_name),
        );
    }
}

pub fn reset_operator_profile_session<EvaluationPage>() -> SessionState<EvaluationPage> {
    profile::set_session_play_style(OPERATOR_RESET_STYLE);
    profile::set_session_play_mode(PlayMode::Regular);
    profile::set_session_player_side(PlayerSide::P1);
    profile::set_session_joined(false, false);
    profile::set_session_music_rate(1.0);
    profile::set_session_timing_tick_mode(TimingTickMode::Off);
    profile::set_fast_profile_switch_from_select_music(false);

    let preferred = profile::preferred_difficulty_for_side(PlayerSide::P1, OPERATOR_RESET_STYLE);
    SessionState::new(preferred, profile::combo_carry())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo_context(play_style: PlayStyle) -> GameplayComboCarryContext {
        GameplayComboCarryContext {
            autoplay_used: false,
            play_style,
            active_side: PlayerSide::P2,
            player_combos: [Some(123), Some(456)],
        }
    }

    #[test]
    fn versus_combo_carry_updates_both_sides() {
        assert_eq!(
            gameplay_combo_carry_updates(combo_context(PlayStyle::Versus)),
            [
                Some(ComboCarryUpdate {
                    side: PlayerSide::P1,
                    combo: 123,
                }),
                Some(ComboCarryUpdate {
                    side: PlayerSide::P2,
                    combo: 456,
                }),
            ]
        );
    }

    #[test]
    fn profile_selection_session_follows_confirmed_joined_sides() {
        assert_eq!(
            profile_selection_session_plan(PlayStyle::Single, true, true),
            ProfileSelectionSessionPlan {
                active_side: PlayerSide::P1,
                p1_joined: true,
                p2_joined: true,
                play_style: PlayStyle::Versus,
            }
        );
        assert_eq!(
            profile_selection_session_plan(PlayStyle::Versus, false, true),
            ProfileSelectionSessionPlan {
                active_side: PlayerSide::P2,
                p1_joined: false,
                p2_joined: true,
                play_style: PlayStyle::Single,
            }
        );
        assert_eq!(
            profile_selection_session_plan(PlayStyle::Double, true, false),
            ProfileSelectionSessionPlan {
                active_side: PlayerSide::P1,
                p1_joined: true,
                p2_joined: false,
                play_style: PlayStyle::Double,
            }
        );
    }

    #[test]
    fn single_and_double_combo_carry_use_the_active_side() {
        for play_style in [PlayStyle::Single, PlayStyle::Double] {
            assert_eq!(
                gameplay_combo_carry_updates(combo_context(play_style)),
                [
                    Some(ComboCarryUpdate {
                        side: PlayerSide::P2,
                        combo: 123,
                    }),
                    None,
                ]
            );
        }
    }

    #[test]
    fn combo_carry_ignores_autoplay_and_missing_players() {
        assert_eq!(
            gameplay_combo_carry_updates(GameplayComboCarryContext {
                autoplay_used: true,
                ..combo_context(PlayStyle::Versus)
            }),
            [None, None]
        );
        assert_eq!(
            gameplay_combo_carry_updates(GameplayComboCarryContext {
                player_combos: [None, Some(456)],
                ..combo_context(PlayStyle::Single)
            }),
            [None, None]
        );
    }

    #[test]
    fn course_history_updates_both_versus_sides_or_the_active_side() {
        assert_eq!(
            course_last_played_sides(PlayStyle::Versus, PlayerSide::P2),
            [Some(PlayerSide::P1), Some(PlayerSide::P2)]
        );
        assert_eq!(
            course_last_played_sides(PlayStyle::Single, PlayerSide::P2),
            [Some(PlayerSide::P2), None]
        );
        assert_eq!(
            course_last_played_sides(PlayStyle::Double, PlayerSide::P1),
            [Some(PlayerSide::P1), None]
        );
    }
}
