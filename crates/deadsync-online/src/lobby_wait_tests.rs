use super::{
    EvaluationLobbyAction, EvaluationLobbySession, EvaluationPlayerStatsFacts, LobbyWaitReason,
    LocalLobbyPlayers, SelectMusicLobbySession,
};
use crate::gameplay_lobby::LobbyRuntimeView;
use crate::lobbies::{
    ConnectionState, JoinedLobby, LobbyJudgments, LobbyPlayer, LobbySongInfo, MachinePlayerStats,
    PublicLobby, ResponseStatus, Snapshot,
};
use deadsync_core::input::InputSource;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayerSide;
use deadsync_rules::timing::WindowCounts;
use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

const LOCAL_P1: LocalLobbyPlayers = LocalLobbyPlayers {
    joined: [true, false],
    player_side: PlayerSide::P1,
};

fn player(screen: &str) -> LobbyPlayer {
    LobbyPlayer {
        label: screen.into(),
        ready: false,
        screen_name: screen.into(),
        judgments: None,
        score: None,
        ex_score: None,
    }
}

fn view(screens: &[&str], selected_song: bool) -> LobbyRuntimeView {
    LobbyRuntimeView {
        snapshot: Arc::new(Snapshot {
            connection: ConnectionState::Connected,
            joined_lobby: Some(JoinedLobby {
                code: "WAIT".into(),
                players: screens.iter().map(|screen| player(screen)).collect(),
                song_info: selected_song.then(|| LobbySongInfo {
                    song_path: "Songs/Pack/Song".into(),
                    ..Default::default()
                }),
            }),
            ..Snapshot::default()
        }),
        disconnect_hold_seconds: 5.0,
        ..LobbyRuntimeView::default()
    }
}

fn locked_view() -> LobbyRuntimeView {
    view(
        &[
            "ScreenGameplay",
            "ScreenEvaluationStage",
            "ScreenSelectMusic",
        ],
        true,
    )
}

fn event(action: VirtualAction, pressed: bool, at: Instant) -> InputEvent {
    InputEvent::new(action, 0, pressed, InputSource::Keyboard, at, 0, at, at)
}

fn evaluation_update(
    session: &mut EvaluationLobbySession,
    local: LocalLobbyPlayers,
    now: Instant,
) -> Vec<EvaluationLobbyAction> {
    let mut actions = Vec::new();
    session.update_at(local, now, |_| None, |action| actions.push(action));
    actions
}

fn requested_disconnect(actions: &[EvaluationLobbyAction]) -> bool {
    actions
        .iter()
        .any(|action| matches!(action, EvaluationLobbyAction::Disconnect))
}

#[test]
fn absent_and_local_only_lobbies_bypass_reconnect_for_both_screens() {
    for (local, expected_count) in [
        (LOCAL_P1, 1),
        (
            LocalLobbyPlayers {
                joined: [false, true],
                player_side: PlayerSide::P2,
            },
            1,
        ),
        (
            LocalLobbyPlayers {
                joined: [false, false],
                player_side: PlayerSide::P2,
            },
            1,
        ),
        (
            LocalLobbyPlayers {
                joined: [true, true],
                player_side: PlayerSide::P1,
            },
            2,
        ),
    ] {
        assert_eq!(local.count(), expected_count);
        let absent = LobbyRuntimeView {
            reconnect_status_text: Some("Reconnecting...".into()),
            ..Default::default()
        };
        assert_eq!(
            EvaluationLobbySession::new(absent.clone()).wait_reason(local),
            None
        );
        assert_eq!(
            SelectMusicLobbySession::new(absent).wait_reason(local),
            None
        );

        for count in 0..=expected_count {
            let mut local_only = view(&vec!["ScreenGameplay"; count], false);
            local_only.reconnect_status_text = Some("Reconnecting...".into());
            assert_eq!(
                EvaluationLobbySession::new(local_only.clone()).wait_reason(local),
                None,
                "local lobby of {count} players should not wait"
            );
            assert_eq!(
                SelectMusicLobbySession::new(local_only).wait_reason(local),
                None
            );
        }
    }
}

#[test]
fn evaluation_waits_for_gameplay_case_insensitively_with_reconnect_first() {
    for selected_song in [false, true] {
        for (screen, expected) in [
            ("sCrEeNgAmEpLaY", Some(LobbyWaitReason::WaitingForGameplay)),
            ("ScreenEvaluationStage", None),
            ("ScreenPlayerOptions", None),
            ("ScreenSelectMusic", None),
            ("ScreenGameplayExtra", None),
        ] {
            let mut runtime = view(&["ScreenEvaluationStage", screen], selected_song);
            assert_eq!(
                EvaluationLobbySession::new(runtime.clone()).wait_reason(LOCAL_P1),
                expected,
                "remote screen {screen}, selected song {selected_song}"
            );
            runtime.reconnect_status_text = Some(String::new());
            assert_eq!(
                EvaluationLobbySession::new(runtime).wait_reason(LOCAL_P1),
                Some(LobbyWaitReason::Reconnecting)
            );
        }
    }
}

#[test]
fn select_music_wait_reasons_preserve_screen_and_selected_song_precedence() {
    use LobbyWaitReason::{Reconnecting, WaitingForEvaluation, WaitingForGameplay, WaitingForSync};
    type WaitCase<'a> = (
        &'a [&'a str],
        bool,
        Option<&'a str>,
        Option<LobbyWaitReason>,
    );
    let cases: &[WaitCase<'_>] = &[
        (
            &["ScreenSelectMusic", "sCrEeNsElEcTmUsIc"],
            false,
            None,
            None,
        ),
        (
            &["ScreenSelectMusic", "sCrEeNgAmEpLaY"],
            false,
            None,
            Some(WaitingForGameplay),
        ),
        (&["ScreenSelectMusic", "ScreenGameplay"], true, None, None),
        (
            &["ScreenSelectMusic", "sCrEeNeVaLuAtIoNsTaGe"],
            false,
            None,
            Some(WaitingForEvaluation),
        ),
        (
            &["ScreenSelectMusic", "ScreenEvaluationStage"],
            true,
            None,
            Some(WaitingForEvaluation),
        ),
        (
            &["ScreenGameplay", "ScreenEvaluationStage"],
            true,
            None,
            Some(WaitingForEvaluation),
        ),
        (
            &["ScreenSelectMusic", "ScreenPlayerOptions"],
            false,
            None,
            Some(WaitingForSync),
        ),
        (
            &["ScreenSelectMusic", "ScreenPlayerOptions"],
            true,
            None,
            None,
        ),
        (
            &["ScreenSelectMusic", "UnknownScreen"],
            false,
            None,
            Some(WaitingForSync),
        ),
        (
            &["ScreenSelectMusic", "ScreenEvaluationStage"],
            true,
            Some("Reconnecting..."),
            Some(Reconnecting),
        ),
        (
            &["ScreenSelectMusic", "ScreenSelectMusic"],
            true,
            Some(""),
            Some(Reconnecting),
        ),
    ];
    for &(screens, selected_song, reconnect, expected) in cases {
        let mut runtime = view(screens, selected_song);
        runtime.reconnect_status_text = reconnect.map(str::to_owned);
        assert_eq!(
            SelectMusicLobbySession::new(runtime).wait_reason(LOCAL_P1),
            expected,
            "screens {screens:?}, selected song {selected_song}, reconnect {reconnect:?}"
        );
    }
}

#[test]
fn selected_song_ignores_ready_scores_judgments_and_possible_mines() {
    // ITGmania can report possible mines as hits before ready-up. Those facts
    // must not block arrival in gameplay once the lobby has selected a song.
    for player_count in [2, 4] {
        for gameplay_count in 1..player_count {
            for ready in [false, true] {
                for score in [0.0, 1.0] {
                    let mut runtime = view(&[], true);
                    let mut remote = player("ScreenGameplay");
                    remote.ready = ready;
                    remote.judgments = Some(LobbyJudgments {
                        fantastics: 1,
                        total_steps: 100,
                        mines_hit: 20,
                        total_mines: 20,
                        ..Default::default()
                    });
                    remote.score = Some(score);
                    remote.ex_score = Some(score);
                    let players = &mut Arc::make_mut(&mut runtime.snapshot)
                        .joined_lobby
                        .as_mut()
                        .unwrap()
                        .players;
                    players.resize(gameplay_count, remote);
                    players.resize(player_count, player("ScreenSelectMusic"));
                    assert_eq!(
                        SelectMusicLobbySession::new(runtime).wait_reason(LOCAL_P1),
                        None,
                        "{gameplay_count}/{player_count} in gameplay, ready {ready}, score {score}"
                    );
                }
            }
        }
    }
}

#[test]
fn locked_input_consumes_edges_but_only_active_start_owns_holds() {
    let now = Instant::now();
    for (local, active_sides) in [
        (LOCAL_P1, [true, false]),
        (
            LocalLobbyPlayers {
                joined: [false, true],
                player_side: PlayerSide::P1,
            },
            [false, true],
        ),
        (
            LocalLobbyPlayers {
                joined: [true, true],
                player_side: PlayerSide::P1,
            },
            [true, true],
        ),
        (
            LocalLobbyPlayers {
                joined: [false, false],
                player_side: PlayerSide::P1,
            },
            [true, false],
        ),
        (
            LocalLobbyPlayers {
                joined: [false, false],
                player_side: PlayerSide::P2,
            },
            [false, true],
        ),
    ] {
        let mut evaluation = EvaluationLobbySession::new(locked_view());
        let mut selection = SelectMusicLobbySession::new(locked_view());
        for action in [VirtualAction::p1_left, VirtualAction::p2_right] {
            for pressed in [true, false] {
                let input = event(action, pressed, now);
                assert!(evaluation.handle_input(local, &input));
                assert!(selection.handle_input(local, &input));
            }
        }
        assert_eq!(evaluation.disconnect_hold_elapsed_at(now), None);
        assert_eq!(selection.disconnect_hold_elapsed_at(now), None);

        for (index, action) in [(0, VirtualAction::p1_start), (1, VirtualAction::p2_start)] {
            let press = event(action, true, now);
            assert!(evaluation.handle_input(local, &press));
            assert!(selection.handle_input(local, &press));
            let expected = active_sides[index].then_some(0.0);
            assert_eq!(evaluation.disconnect_hold_elapsed_at(now), expected);
            assert_eq!(selection.disconnect_hold_elapsed_at(now), expected);
            let release = event(action, false, now);
            assert!(evaluation.handle_input(local, &release));
            assert!(selection.handle_input(local, &release));
            assert_eq!(evaluation.disconnect_hold_elapsed_at(now), None);
            assert_eq!(selection.disconnect_hold_elapsed_at(now), None);
        }
    }

    let input = event(VirtualAction::p1_start, true, now);
    let mut evaluation = EvaluationLobbySession::default();
    let mut selection = SelectMusicLobbySession::default();
    assert!(!evaluation.handle_input(LOCAL_P1, &input));
    assert!(!selection.handle_input(LOCAL_P1, &input));
    assert_eq!(evaluation.disconnect_hold_elapsed_at(now), None);
    assert_eq!(selection.disconnect_hold_elapsed_at(now), None);
}

#[test]
fn release_and_repress_reset_the_hold_deadline_for_both_screens() {
    let now = Instant::now();
    let mut evaluation = EvaluationLobbySession::new(locked_view());
    let mut selection = SelectMusicLobbySession::new(locked_view());
    for (seconds, pressed) in [(0, true), (4, false)] {
        let input = event(
            VirtualAction::p1_start,
            pressed,
            now + Duration::from_secs(seconds),
        );
        assert!(evaluation.handle_input(LOCAL_P1, &input));
        assert!(selection.handle_input(LOCAL_P1, &input));
    }
    let after_release = now + Duration::from_secs(6);
    assert!(!requested_disconnect(&evaluation_update(
        &mut evaluation,
        LOCAL_P1,
        after_release
    )));
    assert!(
        !selection
            .update_at(LOCAL_P1, after_release)
            .disconnect_requested
    );
    for seconds in [7, 8] {
        let input = event(
            VirtualAction::p1_start,
            true,
            now + Duration::from_secs(seconds),
        );
        evaluation.handle_input(LOCAL_P1, &input);
        selection.handle_input(LOCAL_P1, &input);
    }
    let before_timestamp = now + Duration::from_secs(7);
    assert_eq!(
        evaluation.disconnect_hold_elapsed_at(before_timestamp),
        Some(0.0)
    );
    assert_eq!(
        selection.disconnect_hold_elapsed_at(before_timestamp),
        Some(0.0)
    );
    let old_deadline = now + Duration::from_secs(12);
    assert!(!requested_disconnect(&evaluation_update(
        &mut evaluation,
        LOCAL_P1,
        old_deadline
    )));
    assert!(
        !selection
            .update_at(LOCAL_P1, old_deadline)
            .disconnect_requested
    );
    let new_deadline = now + Duration::from_secs(13);
    assert!(requested_disconnect(&evaluation_update(
        &mut evaluation,
        LOCAL_P1,
        new_deadline
    )));
    assert!(
        selection
            .update_at(LOCAL_P1, new_deadline)
            .disconnect_requested
    );
}

#[test]
fn earliest_active_hold_disconnects_at_the_exact_deadline() {
    let now = Instant::now();
    let local = LocalLobbyPlayers {
        joined: [true, true],
        player_side: PlayerSide::P1,
    };
    let mut evaluation = EvaluationLobbySession::new(locked_view());
    let mut selection = SelectMusicLobbySession::new(locked_view());
    for (action, seconds) in [(VirtualAction::p1_start, 2), (VirtualAction::p2_start, 0)] {
        let input = event(action, true, now + Duration::from_secs(seconds));
        assert!(evaluation.handle_input(local, &input));
        assert!(selection.handle_input(local, &input));
    }
    let before = now + Duration::from_millis(4999);
    assert!(!requested_disconnect(&evaluation_update(
        &mut evaluation,
        local,
        before
    )));
    assert!(!selection.update_at(local, before).disconnect_requested);
    let deadline = now + Duration::from_secs(5);
    assert_eq!(evaluation.disconnect_hold_elapsed_at(deadline), Some(5.0));
    assert_eq!(selection.disconnect_hold_elapsed_at(deadline), Some(5.0));
    assert!(requested_disconnect(&evaluation_update(
        &mut evaluation,
        local,
        deadline
    )));
    let update = selection.update_at(local, deadline);
    assert!(update.waiting && update.disconnect_requested);
    assert_eq!(evaluation.disconnect_hold_elapsed_at(deadline), None);
    assert_eq!(selection.disconnect_hold_elapsed_at(deadline), None);
}

#[test]
fn live_local_context_and_snapshot_refresh_preserve_an_existing_hold() {
    let now = Instant::now();
    let runtime = view(&["ScreenGameplay", "ScreenEvaluationStage"], true);
    let mut evaluation = EvaluationLobbySession::new(runtime.clone());
    let mut selection = SelectMusicLobbySession::new(runtime);
    let original_press = event(VirtualAction::p1_start, true, now);
    assert!(evaluation.handle_input(LOCAL_P1, &original_press));
    assert!(selection.handle_input(LOCAL_P1, &original_press));

    let mut refreshed = view(&["ScreenGameplay", "ScreenEvaluationStage"], true);
    Arc::make_mut(&mut refreshed.snapshot)
        .joined_lobby
        .as_mut()
        .unwrap()
        .code = "REFRESHED".into();
    let refreshed_snapshot = refreshed.snapshot.clone();
    evaluation.sync_runtime_view(refreshed.clone());
    selection.sync_runtime_view(refreshed);
    assert!(Arc::ptr_eq(
        &evaluation.runtime_view().snapshot,
        &refreshed_snapshot
    ));
    assert!(Arc::ptr_eq(
        &selection.runtime_view().snapshot,
        &refreshed_snapshot
    ));
    let after_refresh = now + Duration::from_secs(1);
    assert_eq!(
        evaluation.disconnect_hold_elapsed_at(after_refresh),
        Some(1.0)
    );
    assert_eq!(
        selection.disconnect_hold_elapsed_at(after_refresh),
        Some(1.0)
    );

    // Query current local membership without advancing an unlocked frame. That
    // frame's deliberate hold cleanup is exercised by the following test.
    let both_local = LocalLobbyPlayers {
        joined: [true, true],
        player_side: PlayerSide::P1,
    };
    assert_eq!(evaluation.wait_reason(both_local), None);
    assert_eq!(selection.wait_reason(both_local), None);
    let only_p2 = LocalLobbyPlayers {
        joined: [false, true],
        player_side: PlayerSide::P2,
    };
    assert_eq!(
        evaluation.wait_reason(only_p2),
        Some(LobbyWaitReason::WaitingForGameplay)
    );
    assert_eq!(
        selection.wait_reason(only_p2),
        Some(LobbyWaitReason::WaitingForEvaluation)
    );

    let inactive_release = event(VirtualAction::p1_start, false, now + Duration::from_secs(2));
    assert!(evaluation.handle_input(only_p2, &inactive_release));
    assert!(selection.handle_input(only_p2, &inactive_release));
    assert_eq!(
        evaluation.disconnect_hold_elapsed_at(inactive_release.timestamp),
        Some(2.0)
    );
    assert_eq!(
        selection.disconnect_hold_elapsed_at(inactive_release.timestamp),
        Some(2.0)
    );
    let later_press = event(VirtualAction::p2_start, true, now + Duration::from_secs(3));
    assert!(evaluation.handle_input(only_p2, &later_press));
    assert!(selection.handle_input(only_p2, &later_press));

    let before = now + Duration::from_millis(4999);
    assert!(!requested_disconnect(&evaluation_update(
        &mut evaluation,
        only_p2,
        before
    )));
    assert!(!selection.update_at(only_p2, before).disconnect_requested);
    let original_deadline = now + Duration::from_secs(5);
    assert_eq!(
        evaluation.disconnect_hold_elapsed_at(original_deadline),
        Some(5.0)
    );
    assert_eq!(
        selection.disconnect_hold_elapsed_at(original_deadline),
        Some(5.0)
    );
    assert!(requested_disconnect(&evaluation_update(
        &mut evaluation,
        only_p2,
        original_deadline
    )));
    assert!(
        selection
            .update_at(only_p2, original_deadline)
            .disconnect_requested
    );
    assert_eq!(
        evaluation.disconnect_hold_elapsed_at(original_deadline),
        None
    );
    assert_eq!(
        selection.disconnect_hold_elapsed_at(original_deadline),
        None
    );
}

#[test]
fn unlocked_update_clears_holds_and_a_new_wait_requires_a_new_press() {
    let now = Instant::now();
    let mut evaluation = EvaluationLobbySession::new(locked_view());
    let mut selection = SelectMusicLobbySession::new(locked_view());
    let press = event(VirtualAction::p1_start, true, now);
    evaluation.handle_input(LOCAL_P1, &press);
    selection.handle_input(LOCAL_P1, &press);
    evaluation.sync_runtime_view(LobbyRuntimeView::default());
    selection.sync_runtime_view(LobbyRuntimeView::default());
    let later = now + Duration::from_secs(6);
    assert!(evaluation_update(&mut evaluation, LOCAL_P1, later).is_empty());
    let update = selection.update_at(LOCAL_P1, later);
    assert!(!update.waiting && !update.disconnect_requested);
    assert_eq!(evaluation.disconnect_hold_elapsed_at(later), None);
    assert_eq!(selection.disconnect_hold_elapsed_at(later), None);

    evaluation.sync_runtime_view(locked_view());
    selection.sync_runtime_view(locked_view());
    let rejoined = now + Duration::from_secs(10);
    assert!(!requested_disconnect(&evaluation_update(
        &mut evaluation,
        LOCAL_P1,
        rejoined
    )));
    let update = selection.update_at(LOCAL_P1, rejoined);
    assert!(update.waiting && !update.disconnect_requested);
}

#[test]
fn evaluation_publishes_exact_stats_before_disconnect_and_detaches_its_snapshot() {
    #[derive(Debug, PartialEq)]
    enum Observed {
        Facts(PlayerSide),
        Action(EvaluationLobbyAction),
    }

    let now = Instant::now();
    let mut runtime = locked_view();
    runtime.reconnect_status_text = Some("Reconnecting...".into());
    let snapshot = Arc::make_mut(&mut runtime.snapshot);
    snapshot.available_lobbies.push(PublicLobby {
        code: "OTHER".into(),
        player_count: 2,
        is_password_protected: false,
    });
    snapshot.last_status = Some(ResponseStatus {
        event: "joined".into(),
        success: true,
        message: Some("Connected".into()),
    });
    let shared_snapshot = runtime.snapshot.clone();
    let mut session = EvaluationLobbySession::new(runtime);
    assert!(session.handle_input(LOCAL_P1, &event(VirtualAction::p1_start, true, now)));
    let facts = [
        EvaluationPlayerStatsFacts {
            window_counts: WindowCounts {
                w0: 1,
                w1: 2,
                w2: 3,
                w3: 4,
                w4: 5,
                w5: 6,
                miss: 7,
            },
            total_steps: 28,
            mines_total: 5,
            mines_avoided: 2,
            holds_held: 8,
            holds_total: 9,
            rolls_held: 10,
            rolls_total: 11,
            score_percent: 0.98765,
            ex_score_percent: 76.54321,
        },
        EvaluationPlayerStatsFacts {
            mines_total: 2,
            mines_avoided: 5,
            score_percent: 0.5,
            ex_score_percent: 49.0,
            ..Default::default()
        },
    ];
    let observed = RefCell::new(Vec::new());
    session.update_at(
        LOCAL_P1,
        now + Duration::from_secs(5),
        |side| {
            observed.borrow_mut().push(Observed::Facts(side));
            Some(facts[if side == PlayerSide::P1 { 0 } else { 1 }])
        },
        |action| observed.borrow_mut().push(Observed::Action(action)),
    );
    assert_eq!(
        observed.into_inner(),
        vec![
            Observed::Facts(PlayerSide::P1),
            Observed::Facts(PlayerSide::P2),
            Observed::Action(EvaluationLobbyAction::UpdateMachineStats {
                p1_ready: true,
                p2_ready: true,
                p1_stats: Some(MachinePlayerStats {
                    judgments: Some(LobbyJudgments {
                        fantastic_plus: 1,
                        fantastics: 2,
                        excellents: 3,
                        greats: 4,
                        decents: 5,
                        way_offs: 6,
                        misses: 7,
                        total_steps: 28,
                        mines_hit: 3,
                        total_mines: 5,
                        holds_held: 8,
                        total_holds: 9,
                        rolls_held: 10,
                        total_rolls: 11,
                    }),
                    score: Some(98.765),
                    ex_score: Some(76.54321),
                }),
                p2_stats: Some(MachinePlayerStats {
                    judgments: Some(LobbyJudgments {
                        mines_hit: 0,
                        total_mines: 2,
                        ..Default::default()
                    }),
                    score: Some(50.0),
                    ex_score: Some(49.0),
                }),
            }),
            Observed::Action(EvaluationLobbyAction::Disconnect),
        ]
    );
    let after = session.runtime_view();
    assert_eq!(after.snapshot.connection, ConnectionState::Disconnected);
    assert!(after.snapshot.joined_lobby.is_none());
    assert!(after.snapshot.last_status.is_none());
    assert!(after.reconnect_status_text.is_none());
    assert_eq!(
        after.snapshot.available_lobbies,
        shared_snapshot.available_lobbies
    );
    assert!(!Arc::ptr_eq(&after.snapshot, &shared_snapshot));
    assert_eq!(shared_snapshot.connection, ConnectionState::Connected);
    assert!(shared_snapshot.joined_lobby.is_some());
    assert!(shared_snapshot.last_status.is_some());
    assert_eq!(session.wait_reason(LOCAL_P1), None);
    assert!(evaluation_update(&mut session, LOCAL_P1, now + Duration::from_secs(10)).is_empty());
}

#[test]
fn evaluation_reads_stats_only_while_connected_and_joined() {
    let now = Instant::now();
    for connection in [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Error("offline".into()),
    ] {
        for joined in [false, true] {
            let mut runtime = view(&["ScreenEvaluationStage"], false);
            let snapshot = Arc::make_mut(&mut runtime.snapshot);
            snapshot.connection = connection.clone();
            if !joined {
                snapshot.joined_lobby = None;
            }
            let should_publish = connection == ConnectionState::Connected && joined;
            let mut requested_sides = Vec::new();
            let mut actions = Vec::new();
            EvaluationLobbySession::new(runtime).update_at(
                LOCAL_P1,
                now,
                |side| {
                    requested_sides.push(side);
                    None
                },
                |action| actions.push(action),
            );
            if should_publish {
                assert_eq!(requested_sides, [PlayerSide::P1, PlayerSide::P2]);
                assert_eq!(
                    actions,
                    [EvaluationLobbyAction::UpdateMachineStats {
                        p1_ready: true,
                        p2_ready: true,
                        p1_stats: None,
                        p2_stats: None,
                    }]
                );
            } else {
                assert!(
                    requested_sides.is_empty(),
                    "connection {connection:?}, joined {joined}"
                );
                assert!(actions.is_empty());
            }
        }
    }
}

#[test]
fn selection_disconnect_retains_snapshot_and_requests_once_per_completed_hold() {
    let now = Instant::now();
    let mut runtime = locked_view();
    runtime.reconnect_status_text = Some("Reconnecting...".into());
    Arc::make_mut(&mut runtime.snapshot).last_status = Some(ResponseStatus {
        event: "joined".into(),
        success: true,
        message: None,
    });
    let shared_snapshot = runtime.snapshot.clone();
    let mut session = SelectMusicLobbySession::new(runtime);
    assert!(session.handle_input(LOCAL_P1, &event(VirtualAction::p1_start, true, now)));
    let update = session.update_at(LOCAL_P1, now + Duration::from_secs(5));
    assert!(update.waiting && update.disconnect_requested);
    assert!(Arc::ptr_eq(
        &session.runtime_view().snapshot,
        &shared_snapshot
    ));
    assert_eq!(
        session.runtime_view().snapshot.as_ref(),
        shared_snapshot.as_ref()
    );
    assert_eq!(
        session.runtime_view().reconnect_status_text.as_deref(),
        Some("Reconnecting...")
    );
    assert_eq!(
        session.disconnect_hold_elapsed_at(now + Duration::from_secs(5)),
        None
    );

    let update = session.update_at(LOCAL_P1, now + Duration::from_secs(100));
    assert!(update.waiting && !update.disconnect_requested);
    // The shell may still be processing the queued request. A distinct later
    // hold remains available, but elapsed frames alone cannot enqueue it again.
    let press = event(
        VirtualAction::p1_start,
        true,
        now + Duration::from_secs(101),
    );
    assert!(session.handle_input(LOCAL_P1, &press));
    assert!(
        session
            .update_at(LOCAL_P1, now + Duration::from_secs(106))
            .disconnect_requested
    );
    assert!(Arc::ptr_eq(
        &session.runtime_view().snapshot,
        &shared_snapshot
    ));
}
