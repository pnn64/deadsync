use crate::app_config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineFlowScreen {
    Menu,
    SelectProfile,
    SelectColor,
    SelectStyle,
    SelectPlayMode,
    ProfileLoad,
    EvaluationSummary,
    Initials,
    GameOver,
}

pub const fn machine_startup_screen_enabled(cfg: &Config, screen: MachineFlowScreen) -> bool {
    match screen {
        MachineFlowScreen::SelectProfile => cfg.machine_show_select_profile,
        MachineFlowScreen::SelectColor => cfg.machine_show_select_color,
        MachineFlowScreen::SelectStyle => cfg.machine_show_select_style,
        MachineFlowScreen::SelectPlayMode => cfg.machine_show_select_play_mode,
        _ => true,
    }
}

pub fn machine_resolve_startup_target(
    cfg: &Config,
    target: MachineFlowScreen,
) -> MachineFlowScreen {
    const ORDER: [MachineFlowScreen; 4] = [
        MachineFlowScreen::SelectProfile,
        MachineFlowScreen::SelectColor,
        MachineFlowScreen::SelectStyle,
        MachineFlowScreen::SelectPlayMode,
    ];

    let Some(start_idx) = ORDER.iter().position(|screen| *screen == target) else {
        return target;
    };
    ORDER
        .iter()
        .skip(start_idx)
        .copied()
        .find(|screen| machine_startup_screen_enabled(cfg, *screen))
        .unwrap_or(MachineFlowScreen::ProfileLoad)
}

pub const fn machine_first_post_select_target(cfg: &Config) -> MachineFlowScreen {
    if cfg.machine_show_eval_summary {
        MachineFlowScreen::EvaluationSummary
    } else if cfg.machine_show_name_entry {
        MachineFlowScreen::Initials
    } else if cfg.machine_show_gameover {
        MachineFlowScreen::GameOver
    } else {
        MachineFlowScreen::Menu
    }
}

pub const fn machine_resolve_post_select_target(
    cfg: &Config,
    target: MachineFlowScreen,
) -> MachineFlowScreen {
    match target {
        MachineFlowScreen::EvaluationSummary => machine_first_post_select_target(cfg),
        MachineFlowScreen::Initials => {
            if cfg.machine_show_name_entry {
                MachineFlowScreen::Initials
            } else if cfg.machine_show_gameover {
                MachineFlowScreen::GameOver
            } else {
                MachineFlowScreen::Menu
            }
        }
        MachineFlowScreen::GameOver => {
            if cfg.machine_show_gameover {
                MachineFlowScreen::GameOver
            } else {
                MachineFlowScreen::Menu
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::Config;

    #[test]
    fn startup_target_skips_disabled_steps() {
        let cfg = Config {
            machine_show_select_profile: false,
            machine_show_select_color: false,
            machine_show_select_style: true,
            machine_show_select_play_mode: true,
            ..Default::default()
        };

        assert_eq!(
            machine_resolve_startup_target(&cfg, MachineFlowScreen::SelectProfile),
            MachineFlowScreen::SelectStyle
        );
        assert_eq!(
            machine_resolve_startup_target(&cfg, MachineFlowScreen::SelectColor),
            MachineFlowScreen::SelectStyle
        );
    }

    #[test]
    fn startup_target_falls_through_to_profile_load() {
        let cfg = Config {
            machine_show_select_profile: false,
            machine_show_select_color: false,
            machine_show_select_style: false,
            machine_show_select_play_mode: false,
            ..Default::default()
        };

        assert_eq!(
            machine_resolve_startup_target(&cfg, MachineFlowScreen::SelectProfile),
            MachineFlowScreen::ProfileLoad
        );
    }

    #[test]
    fn post_select_target_skips_disabled_screens() {
        let cfg = Config {
            machine_show_eval_summary: false,
            machine_show_name_entry: true,
            machine_show_gameover: true,
            ..Default::default()
        };

        assert_eq!(
            machine_first_post_select_target(&cfg),
            MachineFlowScreen::Initials
        );
        assert_eq!(
            machine_resolve_post_select_target(&cfg, MachineFlowScreen::EvaluationSummary),
            MachineFlowScreen::Initials
        );
    }

    #[test]
    fn post_select_target_keeps_explicit_enabled_target() {
        let cfg = Config {
            machine_show_eval_summary: false,
            machine_show_name_entry: true,
            machine_show_gameover: true,
            ..Default::default()
        };

        assert_eq!(
            machine_resolve_post_select_target(&cfg, MachineFlowScreen::Initials),
            MachineFlowScreen::Initials
        );
        assert_eq!(
            machine_resolve_post_select_target(&cfg, MachineFlowScreen::GameOver),
            MachineFlowScreen::GameOver
        );
    }
}
