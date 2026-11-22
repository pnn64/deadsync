use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use deadsync_theme::ThemeScreenFlow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartPrepareSource {
    Gameplay,
    Evaluation,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayRestartRoute<S> {
    MissingState,
    FastGameplayExit,
    Navigate(S),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FastGameplayRestartPlan {
    pub restart_count: u32,
    pub restart_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayReloadSource {
    Gameplay,
    Evaluation,
    Unavailable,
}

pub fn gameplay_restart_prepare_source<T: ThemeScreenFlow>(
    screen: T::Screen,
    gameplay_state_active: bool,
    evaluation_payload_available: bool,
) -> RestartPrepareSource {
    if gameplay_state_active {
        RestartPrepareSource::Gameplay
    } else if screen == T::SCREENS.results && evaluation_payload_available {
        RestartPrepareSource::Evaluation
    } else {
        RestartPrepareSource::Unavailable
    }
}

pub fn practice_restart_prepare_source<T: ThemeScreenFlow>(
    screen: T::Screen,
    evaluation_payload_available: bool,
) -> RestartPrepareSource {
    if practice_from_eval_allowed::<T>(screen, evaluation_payload_available) {
        RestartPrepareSource::Evaluation
    } else {
        RestartPrepareSource::Unavailable
    }
}

pub fn gameplay_restart_route<T: ThemeScreenFlow>(
    screen: T::Screen,
    restartable_state: bool,
    gameplay_state_active: bool,
) -> GameplayRestartRoute<T::Screen> {
    if !restartable_state {
        GameplayRestartRoute::MissingState
    } else if screen == T::SCREENS.gameplay && gameplay_state_active {
        GameplayRestartRoute::FastGameplayExit
    } else {
        GameplayRestartRoute::Navigate(T::SCREENS.gameplay)
    }
}

pub const fn fast_gameplay_restart_plan(
    current_restart_count: u32,
    already_exiting: bool,
    exit_active_after_begin: bool,
) -> Option<FastGameplayRestartPlan> {
    if !already_exiting && exit_active_after_begin {
        Some(FastGameplayRestartPlan {
            restart_count: current_restart_count.saturating_add(1),
            restart_pending: true,
        })
    } else {
        None
    }
}

pub fn gameplay_reload_source<T: ThemeScreenFlow>(
    screen: T::Screen,
    gameplay_state_active: bool,
    evaluation_payload_available: bool,
) -> GameplayReloadSource {
    if gameplay_state_active {
        GameplayReloadSource::Gameplay
    } else if practice_from_eval_allowed::<T>(screen, evaluation_payload_available) {
        GameplayReloadSource::Evaluation
    } else {
        GameplayReloadSource::Unavailable
    }
}

#[inline(always)]
pub fn practice_from_eval_allowed<T: ThemeScreenFlow>(
    screen: T::Screen,
    evaluation_payload_available: bool,
) -> bool {
    screen == T::SCREENS.results && evaluation_payload_available
}

#[inline(always)]
pub fn practice_reload_allowed<T: ThemeScreenFlow>(screen: T::Screen, practice_state_active: bool) -> bool {
    screen == T::SCREENS.practice && practice_state_active
}

#[inline(always)]
const fn select_restart_steps(
    resolved_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> [usize; MAX_PLAYERS] {
    match play_style {
        PlayStyle::Versus
        | PlayStyle::PumpVersus
        | PlayStyle::Couple
        | PlayStyle::PumpCouple
        | PlayStyle::Routine
        | PlayStyle::PumpRoutine => resolved_steps,
        PlayStyle::Single | PlayStyle::Double | PlayStyle::PumpSingle | PlayStyle::PumpDouble => {
            [resolved_steps[player_side_index(player_side)]; MAX_PLAYERS]
        }
    }
}

pub fn restart_chart_steps(
    song: &SongData,
    chart_hashes: [&str; MAX_PLAYERS],
    chart_type: &str,
    fallback_steps: usize,
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> [usize; MAX_PLAYERS] {
    let resolved_steps = std::array::from_fn(|idx| {
        song.steps_index_for_chart_hash(chart_type, chart_hashes[idx])
            .unwrap_or(fallback_steps)
    });
    select_restart_steps(resolved_steps, play_style, player_side)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_theme::{Theme, ThemeScreenId, ThemeScreens};

    struct TestTheme;

    impl Theme for TestTheme {
        type Screen = &'static str;
        type RuntimeRequest = ();

        fn screen_id(screen: Self::Screen) -> ThemeScreenId {
            ThemeScreenId::new(screen)
        }
    }

    impl ThemeScreenFlow for TestTheme {
        const SCREENS: ThemeScreens<Self::Screen> = ThemeScreens {
            menu: "home",
            gameplay: "play",
            practice: "editor",
            results: "results",
            song_selection: "library",
            course_selection: "courses",
            player_options: "settings",
        };
    }

    #[test]
    fn restart_steps_preserve_versus_and_collapse_single_side() {
        assert_eq!(
            select_restart_steps([2, 4], PlayStyle::Versus, PlayerSide::P1),
            [2, 4],
        );
        assert_eq!(
            select_restart_steps([2, 4], PlayStyle::Single, PlayerSide::P2),
            [4, 4],
        );
        assert_eq!(
            select_restart_steps([2, 4], PlayStyle::Double, PlayerSide::P1),
            [2, 2],
        );
    }

    #[test]
    fn restart_prepare_source_prefers_live_gameplay_then_eval_payload() {
        assert_eq!(
            gameplay_restart_prepare_source::<TestTheme>("play", true, false),
            RestartPrepareSource::Gameplay
        );
        assert_eq!(
            gameplay_restart_prepare_source::<TestTheme>("results", false, true),
            RestartPrepareSource::Evaluation
        );
        assert_eq!(
            gameplay_restart_prepare_source::<TestTheme>("library", false, true),
            RestartPrepareSource::Unavailable
        );
        assert_eq!(
            practice_restart_prepare_source::<TestTheme>("results", true),
            RestartPrepareSource::Evaluation
        );
        assert_eq!(
            practice_restart_prepare_source::<TestTheme>("play", true),
            RestartPrepareSource::Unavailable
        );
    }

    #[test]
    fn gameplay_restart_route_uses_fast_exit_only_from_live_gameplay() {
        assert_eq!(
            gameplay_restart_route::<TestTheme>("play", false, true),
            GameplayRestartRoute::MissingState
        );
        assert_eq!(
            gameplay_restart_route::<TestTheme>("play", true, true),
            GameplayRestartRoute::FastGameplayExit
        );
        assert_eq!(
            gameplay_restart_route::<TestTheme>("results", true, false),
            GameplayRestartRoute::Navigate("play")
        );
    }

    #[test]
    fn fast_restart_marks_session_only_when_exit_newly_starts() {
        assert_eq!(
            fast_gameplay_restart_plan(4, false, true),
            Some(FastGameplayRestartPlan {
                restart_count: 5,
                restart_pending: true,
            })
        );
        assert_eq!(fast_gameplay_restart_plan(4, true, true), None);
        assert_eq!(fast_gameplay_restart_plan(4, false, false), None);
        assert_eq!(
            fast_gameplay_restart_plan(u32::MAX, false, true).map(|plan| plan.restart_count),
            Some(u32::MAX)
        );
    }

    #[test]
    fn reload_and_practice_routes_require_matching_screen_state() {
        assert_eq!(
            gameplay_reload_source::<TestTheme>("play", true, false),
            GameplayReloadSource::Gameplay
        );
        assert_eq!(
            gameplay_reload_source::<TestTheme>("results", false, true),
            GameplayReloadSource::Evaluation
        );
        assert_eq!(
            gameplay_reload_source::<TestTheme>("results", false, false),
            GameplayReloadSource::Unavailable
        );
        assert!(practice_from_eval_allowed::<TestTheme>("results", true));
        assert!(!practice_from_eval_allowed::<TestTheme>("results", false));
        assert!(practice_reload_allowed::<TestTheme>("editor", true));
        assert!(!practice_reload_allowed::<TestTheme>("play", true));
    }
}
