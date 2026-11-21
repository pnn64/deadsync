use super::{
    GameplayExitDestination, GameplayExitRoute, GameplayExitRouteContext,
    resolve_gameplay_exit_route,
};
use crate::{GameplayAction, GameplayExit};

fn gameplay_context() -> GameplayExitRouteContext {
    GameplayExitRouteContext {
        from_gameplay: true,
        ..GameplayExitRouteContext::default()
    }
}

fn all_contexts() -> impl Iterator<Item = GameplayExitRouteContext> {
    (0_u8..32).map(|flags| GameplayExitRouteContext {
        from_gameplay: flags & 1 != 0,
        restart_pending: flags & 2 != 0,
        course_active: flags & 4 != 0,
        course_has_next_stage: flags & 8 != 0,
        all_joined_players_failed: flags & 16 != 0,
    })
}

fn assert_route(
    action: GameplayAction,
    context: GameplayExitRouteContext,
    destination: GameplayExitDestination,
    immediate: bool,
    consume_restart_pending: bool,
) {
    assert_eq!(
        resolve_gameplay_exit_route(action, context),
        Some(GameplayExitRoute {
            destination,
            immediate,
            consume_restart_pending,
        }),
        "action={action:?}, context={context:?}"
    );
}

#[test]
fn restart_redirect_takes_priority_over_course_selection_and_failure() {
    for context in all_contexts().filter(|context| context.from_gameplay && context.restart_pending)
    {
        assert_route(
            GameplayAction::NavigateNoFade(GameplayExit::Cancel),
            context,
            GameplayExitDestination::RestartStage,
            true,
            true,
        );
    }
}

#[test]
fn restart_redirect_requires_gameplay_and_pending_restart() {
    for (context, destination) in [
        (
            GameplayExitRouteContext {
                from_gameplay: false,
                restart_pending: true,
                course_active: true,
                course_has_next_stage: true,
                all_joined_players_failed: false,
            },
            GameplayExitDestination::SongSelection,
        ),
        (gameplay_context(), GameplayExitDestination::SongSelection),
        (
            GameplayExitRouteContext {
                course_active: true,
                ..gameplay_context()
            },
            GameplayExitDestination::CourseSelection,
        ),
    ] {
        assert_route(
            GameplayAction::NavigateNoFade(GameplayExit::Cancel),
            context,
            destination,
            true,
            false,
        );
    }
}

#[test]
fn normal_completion_uses_the_next_stage_fact_independently_of_course_flag() {
    for course_active in [false, true] {
        for restart_pending in [false, true] {
            assert_route(
                GameplayAction::Navigate(GameplayExit::Complete),
                GameplayExitRouteContext {
                    course_active,
                    course_has_next_stage: true,
                    restart_pending,
                    ..gameplay_context()
                },
                GameplayExitDestination::NextCourseStage,
                false,
                false,
            );
        }
    }
}

#[test]
fn failed_or_final_stage_enters_results_without_consuming_restart() {
    for (course_has_next_stage, all_joined_players_failed) in
        [(true, true), (false, false), (false, true)]
    {
        for course_active in [false, true] {
            assert_route(
                GameplayAction::Navigate(GameplayExit::Complete),
                GameplayExitRouteContext {
                    course_active,
                    course_has_next_stage,
                    all_joined_players_failed,
                    restart_pending: true,
                    ..gameplay_context()
                },
                GameplayExitDestination::Results,
                false,
                false,
            );
        }
    }
}

#[test]
fn immediate_completion_always_enters_results() {
    // Failure exits and explicit immediate results navigation must not advance
    // the course, even when the next-stage fact is still true.
    for context in all_contexts() {
        assert_route(
            GameplayAction::NavigateNoFade(GameplayExit::Complete),
            context,
            GameplayExitDestination::Results,
            true,
            false,
        );
    }
}

#[test]
fn course_cancellation_preserves_transition_mode() {
    let context = GameplayExitRouteContext {
        course_active: true,
        ..gameplay_context()
    };
    for (action, immediate) in [
        (GameplayAction::Navigate(GameplayExit::Cancel), false),
        (GameplayAction::NavigateNoFade(GameplayExit::Cancel), true),
    ] {
        assert_route(
            action,
            context,
            GameplayExitDestination::CourseSelection,
            immediate,
            false,
        );
    }
}

#[test]
fn normal_cancellation_never_consumes_a_pending_restart() {
    for (course_active, destination) in [
        (false, GameplayExitDestination::SongSelection),
        (true, GameplayExitDestination::CourseSelection),
    ] {
        assert_route(
            GameplayAction::Navigate(GameplayExit::Cancel),
            GameplayExitRouteContext {
                restart_pending: true,
                course_active,
                course_has_next_stage: true,
                all_joined_players_failed: true,
                ..gameplay_context()
            },
            destination,
            false,
            false,
        );
    }
}

#[test]
fn exits_outside_gameplay_ignore_course_and_restart_state() {
    for context in all_contexts().filter(|context| !context.from_gameplay) {
        for (action, destination, immediate) in [
            (
                GameplayAction::Navigate(GameplayExit::Complete),
                GameplayExitDestination::Results,
                false,
            ),
            (
                GameplayAction::NavigateNoFade(GameplayExit::Complete),
                GameplayExitDestination::Results,
                true,
            ),
            (
                GameplayAction::Navigate(GameplayExit::Cancel),
                GameplayExitDestination::SongSelection,
                false,
            ),
            (
                GameplayAction::NavigateNoFade(GameplayExit::Cancel),
                GameplayExitDestination::SongSelection,
                true,
            ),
        ] {
            assert_route(action, context, destination, immediate, false);
        }
    }
}

#[test]
fn no_action_and_restart_requests_do_not_resolve_completed_exits() {
    for context in all_contexts() {
        for action in [GameplayAction::None, GameplayAction::Restart] {
            assert_eq!(
                resolve_gameplay_exit_route(action, context),
                None,
                "action={action:?}, context={context:?}"
            );
        }
    }
}
