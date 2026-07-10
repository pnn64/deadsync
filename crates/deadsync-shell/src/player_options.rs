use crate::Command;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use deadsync_screens::player_options::{SpeedMod, scroll_speed_for_mod};

pub struct PlayerOptionsPersistPlan {
    pub commands: Vec<Command>,
    pub preferred_difficulty_index: usize,
}

pub fn player_options_persist_plan(
    speed_mod: &[SpeedMod; MAX_PLAYERS],
    chart_difficulty_index: [usize; MAX_PLAYERS],
    music_rate: f32,
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> PlayerOptionsPersistPlan {
    let mut commands = Vec::with_capacity(MAX_PLAYERS + 2);
    match play_style {
        PlayStyle::Versus => {
            for (idx, side) in [(0, PlayerSide::P1), (1, PlayerSide::P2)] {
                commands.push(Command::UpdateScrollSpeed {
                    side,
                    setting: scroll_speed_for_mod(&speed_mod[idx]),
                });
            }
        }
        PlayStyle::Single | PlayStyle::Double => {
            let idx = player_side_index(player_side);
            commands.push(Command::UpdateScrollSpeed {
                side: player_side,
                setting: scroll_speed_for_mod(&speed_mod[idx]),
            });
        }
    }

    commands.push(Command::UpdateSessionMusicRate(music_rate));
    let preferred_difficulty_index = match play_style {
        PlayStyle::Versus => chart_difficulty_index[0],
        PlayStyle::Single | PlayStyle::Double => {
            chart_difficulty_index[player_side_index(player_side)]
        }
    };
    commands.push(Command::UpdatePreferredDifficulty(
        preferred_difficulty_index,
    ));

    PlayerOptionsPersistPlan {
        commands,
        preferred_difficulty_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_screens::player_options::SpeedModType;

    fn speed_mod(mod_type: SpeedModType, value: f32) -> SpeedMod {
        SpeedMod { mod_type, value }
    }

    #[test]
    fn versus_persists_both_sides_and_p1_difficulty() {
        let plan = player_options_persist_plan(
            &[
                speed_mod(SpeedModType::X, 1.5),
                speed_mod(SpeedModType::M, 600.0),
            ],
            [3, 5],
            1.1,
            PlayStyle::Versus,
            PlayerSide::P1,
        );
        assert_eq!(plan.commands.len(), 4);
        assert!(matches!(
            plan.commands[0],
            Command::UpdateScrollSpeed {
                side: PlayerSide::P1,
                setting: ScrollSpeedSetting::XMod(1.5),
            }
        ));
        assert!(matches!(
            plan.commands[1],
            Command::UpdateScrollSpeed {
                side: PlayerSide::P2,
                setting: ScrollSpeedSetting::MMod(600.0),
            }
        ));
        assert_eq!(plan.preferred_difficulty_index, 3);
    }

    #[test]
    fn single_p2_persists_the_physical_side_slot() {
        let plan = player_options_persist_plan(
            &[
                speed_mod(SpeedModType::C, 300.0),
                speed_mod(SpeedModType::M, 725.0),
            ],
            [2, 6],
            1.0,
            PlayStyle::Single,
            PlayerSide::P2,
        );
        assert_eq!(plan.commands.len(), 3);
        assert!(matches!(
            plan.commands[0],
            Command::UpdateScrollSpeed {
                side: PlayerSide::P2,
                setting: ScrollSpeedSetting::MMod(725.0),
            }
        ));
        assert_eq!(plan.preferred_difficulty_index, 6);
    }
}
