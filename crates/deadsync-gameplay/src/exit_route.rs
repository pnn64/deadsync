//! Semantic routing for completed and cancelled gameplay stages.
//!
//! The application supplies live session facts for each atomic action and maps
//! the returned destination to its screens. This policy owns no presentation,
//! course state or pending-restart mutation.

use crate::{GameplayAction, GameplayExit};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayExitRouteContext {
    pub from_gameplay: bool,
    pub restart_pending: bool,
    pub course_active: bool,
    pub course_has_next_stage: bool,
    pub all_joined_players_failed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayExitDestination {
    Results,
    SongSelection,
    CourseSelection,
    NextCourseStage,
    RestartStage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayExitRoute {
    pub destination: GameplayExitDestination,
    pub immediate: bool,
    pub consume_restart_pending: bool,
}

/// Resolve only completed exit actions. A direct `Restart` request must first
/// run the application's restart-exit protocol; its eventual immediate Cancel
/// consumes the pending restart and returns to the stage.
#[must_use]
pub const fn resolve_gameplay_exit_route(
    action: GameplayAction,
    context: GameplayExitRouteContext,
) -> Option<GameplayExitRoute> {
    let (exit, immediate) = match action {
        GameplayAction::Navigate(exit) => (exit, false),
        GameplayAction::NavigateNoFade(exit) => (exit, true),
        GameplayAction::None | GameplayAction::Restart => return None,
    };

    let mut consume_restart_pending = false;
    let destination = match exit {
        GameplayExit::Complete => {
            // Immediate completion goes directly to results. Normal completion
            // follows the next-stage fact independently of course_active.
            if context.from_gameplay
                && !immediate
                && context.course_has_next_stage
                && !context.all_joined_players_failed
            {
                GameplayExitDestination::NextCourseStage
            } else {
                GameplayExitDestination::Results
            }
        }
        GameplayExit::Cancel => {
            if context.from_gameplay && immediate && context.restart_pending {
                consume_restart_pending = true;
                GameplayExitDestination::RestartStage
            } else if context.from_gameplay && context.course_active {
                GameplayExitDestination::CourseSelection
            } else {
                GameplayExitDestination::SongSelection
            }
        }
    };

    Some(GameplayExitRoute {
        destination,
        immediate,
        consume_restart_pending,
    })
}

#[cfg(test)]
#[path = "exit_route_tests.rs"]
mod tests;
