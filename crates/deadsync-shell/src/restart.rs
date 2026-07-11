use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use deadsync_theme_simply_love::views::ScoreInfo;
use std::sync::Arc;

pub struct RestartPayload {
    pub song: Arc<SongData>,
    pub chart_hashes: [String; MAX_PLAYERS],
    pub music_rate: f32,
    pub scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartPrepareSource {
    Gameplay,
    Evaluation,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayRestartRoute {
    MissingState,
    FastGameplayExit,
    Navigate(Screen),
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

pub fn restart_payload_from_eval(
    score_info: &[Option<ScoreInfo>; MAX_PLAYERS],
) -> Option<RestartPayload> {
    let mut song = None;
    let mut chart_hashes = std::array::from_fn(|_| String::new());
    let mut scroll_speed = [ScrollSpeedSetting::default(); MAX_PLAYERS];
    let mut music_rate = None;

    for entry in score_info.iter().flatten() {
        song.get_or_insert_with(|| entry.song.clone());
        let side = player_side_index(entry.side);
        chart_hashes[side] = entry.chart.short_hash.clone();
        scroll_speed[side] = entry.speed_mod;
        if music_rate.is_none() && entry.music_rate.is_finite() && entry.music_rate > 0.0 {
            music_rate = Some(entry.music_rate);
        }
    }

    song.map(|song| RestartPayload {
        song,
        chart_hashes,
        music_rate: music_rate.unwrap_or(1.0),
        scroll_speed,
    })
}

pub const fn gameplay_restart_prepare_source(
    screen: Screen,
    gameplay_state_active: bool,
    evaluation_payload_available: bool,
) -> RestartPrepareSource {
    if gameplay_state_active {
        RestartPrepareSource::Gameplay
    } else if matches!(screen, Screen::Evaluation) && evaluation_payload_available {
        RestartPrepareSource::Evaluation
    } else {
        RestartPrepareSource::Unavailable
    }
}

pub const fn practice_restart_prepare_source(
    screen: Screen,
    evaluation_payload_available: bool,
) -> RestartPrepareSource {
    if matches!(screen, Screen::Evaluation) && evaluation_payload_available {
        RestartPrepareSource::Evaluation
    } else {
        RestartPrepareSource::Unavailable
    }
}

pub const fn gameplay_restart_route(
    screen: Screen,
    restartable_state: bool,
    gameplay_state_active: bool,
) -> GameplayRestartRoute {
    if !restartable_state {
        GameplayRestartRoute::MissingState
    } else if matches!(screen, Screen::Gameplay) && gameplay_state_active {
        GameplayRestartRoute::FastGameplayExit
    } else {
        GameplayRestartRoute::Navigate(Screen::Gameplay)
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

pub const fn gameplay_reload_source(
    screen: Screen,
    gameplay_state_active: bool,
    evaluation_payload_available: bool,
) -> GameplayReloadSource {
    if gameplay_state_active {
        GameplayReloadSource::Gameplay
    } else if matches!(screen, Screen::Evaluation) && evaluation_payload_available {
        GameplayReloadSource::Evaluation
    } else {
        GameplayReloadSource::Unavailable
    }
}

#[inline(always)]
pub const fn practice_from_eval_allowed(
    screen: Screen,
    evaluation_payload_available: bool,
) -> bool {
    matches!(screen, Screen::Evaluation) && evaluation_payload_available
}

#[inline(always)]
pub const fn practice_reload_allowed(screen: Screen, practice_state_active: bool) -> bool {
    matches!(screen, Screen::Practice) && practice_state_active
}

#[inline(always)]
fn select_restart_steps(
    resolved_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> [usize; MAX_PLAYERS] {
    match play_style {
        PlayStyle::Versus => resolved_steps,
        PlayStyle::Single | PlayStyle::Double => {
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
            gameplay_restart_prepare_source(Screen::Gameplay, true, false),
            RestartPrepareSource::Gameplay
        );
        assert_eq!(
            gameplay_restart_prepare_source(Screen::Evaluation, false, true),
            RestartPrepareSource::Evaluation
        );
        assert_eq!(
            gameplay_restart_prepare_source(Screen::SelectMusic, false, true),
            RestartPrepareSource::Unavailable
        );
        assert_eq!(
            practice_restart_prepare_source(Screen::Evaluation, true),
            RestartPrepareSource::Evaluation
        );
        assert_eq!(
            practice_restart_prepare_source(Screen::Gameplay, true),
            RestartPrepareSource::Unavailable
        );
    }

    #[test]
    fn gameplay_restart_route_uses_fast_exit_only_from_live_gameplay() {
        assert_eq!(
            gameplay_restart_route(Screen::Gameplay, false, true),
            GameplayRestartRoute::MissingState
        );
        assert_eq!(
            gameplay_restart_route(Screen::Gameplay, true, true),
            GameplayRestartRoute::FastGameplayExit
        );
        assert_eq!(
            gameplay_restart_route(Screen::Evaluation, true, false),
            GameplayRestartRoute::Navigate(Screen::Gameplay)
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
            gameplay_reload_source(Screen::Gameplay, true, false),
            GameplayReloadSource::Gameplay
        );
        assert_eq!(
            gameplay_reload_source(Screen::Evaluation, false, true),
            GameplayReloadSource::Evaluation
        );
        assert_eq!(
            gameplay_reload_source(Screen::Evaluation, false, false),
            GameplayReloadSource::Unavailable
        );
        assert!(practice_from_eval_allowed(Screen::Evaluation, true));
        assert!(!practice_from_eval_allowed(Screen::Evaluation, false));
        assert!(practice_reload_allowed(Screen::Practice, true));
        assert!(!practice_reload_allowed(Screen::Gameplay, true));
    }
}
