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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppTransitionScreen {
    Menu,
    Options,
    SelectProfile,
    SelectColor,
    SelectStyle,
    Mappings,
    TestLights,
    OverscanAdjustment,
    SmxAssignPads,
    ManageLocalProfiles,
    Input,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCommandKind {
    ExitNow,
    Shutdown,
    SetBanner,
    SetCdTitle,
    SetPackBanner,
    SetWheelItemBackgrounds,
    SetDensityGraph,
    FetchOnlineGrade,
    PlayMusic,
    StopMusic,
    SetDynamicBackground,
    UpdateScrollSpeed,
    UpdateSessionMusicRate,
    UpdatePreferredDifficulty,
    UpdateLastPlayed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCommandTimingLog {
    None,
    CommandTiming,
    FrameCost,
    Slow,
}

pub const fn app_command_label(kind: AppCommandKind) -> &'static str {
    match kind {
        AppCommandKind::ExitNow => "ExitNow",
        AppCommandKind::Shutdown => "Shutdown",
        AppCommandKind::SetBanner => "SetBanner",
        AppCommandKind::SetCdTitle => "SetCdTitle",
        AppCommandKind::SetPackBanner => "SetPackBanner",
        AppCommandKind::SetWheelItemBackgrounds => "SetWheelItemBackgrounds",
        AppCommandKind::SetDensityGraph => "SetDensityGraph",
        AppCommandKind::FetchOnlineGrade => "FetchOnlineGrade",
        AppCommandKind::PlayMusic => "PlayMusic",
        AppCommandKind::StopMusic => "StopMusic",
        AppCommandKind::SetDynamicBackground => "SetDynamicBackground",
        AppCommandKind::UpdateScrollSpeed => "UpdateScrollSpeed",
        AppCommandKind::UpdateSessionMusicRate => "UpdateSessionMusicRate",
        AppCommandKind::UpdatePreferredDifficulty => "UpdatePreferredDifficulty",
        AppCommandKind::UpdateLastPlayed => "UpdateLastPlayed",
    }
}

pub const fn app_command_logs_frame_cost(kind: AppCommandKind) -> bool {
    matches!(
        kind,
        AppCommandKind::SetBanner
            | AppCommandKind::SetCdTitle
            | AppCommandKind::SetPackBanner
            | AppCommandKind::SetWheelItemBackgrounds
            | AppCommandKind::SetDensityGraph
            | AppCommandKind::SetDynamicBackground
            | AppCommandKind::PlayMusic
    )
}

pub fn app_command_timing_log(kind: AppCommandKind, elapsed_ms: f64) -> AppCommandTimingLog {
    if elapsed_ms >= 100.0 {
        AppCommandTimingLog::Slow
    } else if elapsed_ms >= 16.7 {
        AppCommandTimingLog::FrameCost
    } else if app_command_logs_frame_cost(kind) {
        AppCommandTimingLog::CommandTiming
    } else {
        AppCommandTimingLog::None
    }
}

pub const fn app_screen_actor_fades(screen: AppTransitionScreen) -> bool {
    matches!(
        screen,
        AppTransitionScreen::Menu
            | AppTransitionScreen::Options
            | AppTransitionScreen::ManageLocalProfiles
            | AppTransitionScreen::Mappings
            | AppTransitionScreen::Input
            | AppTransitionScreen::TestLights
            | AppTransitionScreen::OverscanAdjustment
            | AppTransitionScreen::SmxAssignPads
            | AppTransitionScreen::SelectProfile
            | AppTransitionScreen::SelectColor
    )
}

pub const fn app_transition_actor_only(from: AppTransitionScreen, to: AppTransitionScreen) -> bool {
    matches!(
        (from, to),
        (
            AppTransitionScreen::Menu,
            AppTransitionScreen::Options
                | AppTransitionScreen::SelectProfile
                | AppTransitionScreen::SelectColor
        ) | (
            AppTransitionScreen::Options
                | AppTransitionScreen::SelectProfile
                | AppTransitionScreen::SelectColor,
            AppTransitionScreen::Menu
        ) | (
            AppTransitionScreen::SelectProfile,
            AppTransitionScreen::SelectColor | AppTransitionScreen::SelectStyle
        ) | (
            AppTransitionScreen::SelectStyle,
            AppTransitionScreen::SelectProfile | AppTransitionScreen::SelectColor
        ) | (
            AppTransitionScreen::SelectColor,
            AppTransitionScreen::SelectStyle
        ) | (
            AppTransitionScreen::Options,
            AppTransitionScreen::Mappings
                | AppTransitionScreen::TestLights
                | AppTransitionScreen::OverscanAdjustment
                | AppTransitionScreen::SmxAssignPads
                | AppTransitionScreen::ManageLocalProfiles
        ) | (
            AppTransitionScreen::Mappings
                | AppTransitionScreen::TestLights
                | AppTransitionScreen::OverscanAdjustment
                | AppTransitionScreen::SmxAssignPads
                | AppTransitionScreen::ManageLocalProfiles,
            AppTransitionScreen::Options
        )
    )
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
    fn app_actor_fade_screen_set_matches_menu_shell_screens() {
        assert!(app_screen_actor_fades(AppTransitionScreen::Menu));
        assert!(app_screen_actor_fades(AppTransitionScreen::Options));
        assert!(app_screen_actor_fades(AppTransitionScreen::SelectProfile));
        assert!(app_screen_actor_fades(AppTransitionScreen::SelectColor));
        assert!(app_screen_actor_fades(
            AppTransitionScreen::ManageLocalProfiles
        ));
        assert!(!app_screen_actor_fades(AppTransitionScreen::SelectStyle));
    }

    #[test]
    fn app_actor_only_transition_set_matches_menu_shell_paths() {
        assert!(app_transition_actor_only(
            AppTransitionScreen::Menu,
            AppTransitionScreen::SelectProfile,
        ));
        assert!(app_transition_actor_only(
            AppTransitionScreen::Options,
            AppTransitionScreen::Mappings,
        ));
        assert!(app_transition_actor_only(
            AppTransitionScreen::SmxAssignPads,
            AppTransitionScreen::Options,
        ));
        assert!(app_transition_actor_only(
            AppTransitionScreen::SelectStyle,
            AppTransitionScreen::SelectColor,
        ));
        assert!(!app_transition_actor_only(
            AppTransitionScreen::Menu,
            AppTransitionScreen::SelectStyle,
        ));
    }

    #[test]
    fn app_command_policy_labels_and_selects_timing_logs() {
        assert_eq!(app_command_label(AppCommandKind::SetBanner), "SetBanner");
        assert!(app_command_logs_frame_cost(AppCommandKind::SetBanner));
        assert!(app_command_logs_frame_cost(AppCommandKind::PlayMusic));
        assert!(!app_command_logs_frame_cost(
            AppCommandKind::UpdateLastPlayed
        ));
        assert_eq!(
            app_command_timing_log(AppCommandKind::UpdateLastPlayed, 1.0),
            AppCommandTimingLog::None,
        );
        assert_eq!(
            app_command_timing_log(AppCommandKind::SetBanner, 1.0),
            AppCommandTimingLog::CommandTiming,
        );
        assert_eq!(
            app_command_timing_log(AppCommandKind::UpdateLastPlayed, 16.7),
            AppCommandTimingLog::FrameCost,
        );
        assert_eq!(
            app_command_timing_log(AppCommandKind::UpdateLastPlayed, 100.0),
            AppCommandTimingLog::Slow,
        );
    }

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
