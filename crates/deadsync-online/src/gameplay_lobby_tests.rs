use super::*;
use crate::lobbies::{ConnectionState, JoinedLobby, LobbyPlayer, Snapshot};
use deadsync_core::input::InputSource;
use deadsync_gameplay as game;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayerSide;
use deadsync_profile_gameplay::GameplayProfile;
use std::path::{Path, PathBuf};
use std::time::Duration;

type Game = game::GameplayRuntimeState<GameplayProfile, (), (), ()>;

struct EmptyVisuals;

impl game::SongLuaRuntimeBuilder<(), (), ()> for EmptyVisuals {
    fn build_song_lua_runtime(
        self,
        params: game::SongLuaRuntimeWindowBuild<'_>,
    ) -> game::SongLuaRuntimeBuildOutput<(), (), ()> {
        (
            Default::default(),
            Default::default(),
            game::SongLuaRuntimeVisuals {
                overlays: vec![],
                foreground_beats: None,
                screen_overlay_index: None,
                overlay_eases: vec![],
                overlay_ease_ranges: vec![],
                overlay_events: vec![],
                background_visual_layers: vec![],
                foreground_visual_layers: vec![],
                player_actors: [(); 2],
                player_events: Default::default(),
                player_judgment_events: Default::default(),
                player_combo_events: Default::default(),
                song_foreground: (),
                song_foreground_events: vec![],
                hidden_players: [false; 2],
                screen_layers: [(); 4],
                screen_layer_events: Default::default(),
                note_hides: Default::default(),
                column_offsets: Default::default(),
                column_splines: Default::default(),
                screen_width: params.screen_width,
                screen_height: params.screen_height,
            },
        )
    }
}

struct Host {
    game: Game,
    lobby: GameplayLobbySession,
}

impl GameplayLobbyHost for Host {
    type Profile = GameplayProfile;
    type OverlayActor = ();
    type CapturedActor = ();
    type StateDelta = ();
    type Effect = GameplayLobbyAction;

    fn lobby_parts(&mut self) -> (&mut Game, &mut GameplayLobbySession) {
        (&mut self.game, &mut self.lobby)
    }

    fn lobby_session(&self) -> &GameplayLobbySession {
        &self.lobby
    }

    fn lobby_effect(action: GameplayLobbyAction) -> GameplayLobbyAction {
        action
    }
}

impl Host {
    fn new(view: LobbyRuntimeView, joined_sides: [bool; 2], side: PlayerSide) -> Self {
        // Build a real gameplay state from an in-memory chart. The audio path is
        // only queued by the controller: no filesystem, network or theme runs.
        let parsed = deadsync_simfile::song::parse_song_bytes(
            b"#TITLE:Lobby lifecycle;\n#BPMS:0=120;\n#NOTEDATA:;\n\
              #STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n\
              #NOTES:\n1000\n0100\n0010\n0001\n;\n",
            "ssc",
            Path::new("lobby-lifecycle.ssc"),
            &deadsync_simfile::song::ParseSongOptions::new(vec![], vec![], vec![]),
            |_| 60.0,
        )
        .unwrap();
        let data = Arc::new(
            deadsync_simfile::cache::build_requested_gameplay_charts(&parsed, &[0], 0.0)
                .unwrap()
                .remove(0),
        );
        let mut song = deadsync_simfile::cache::build_song_meta(parsed, 0.0);
        song.music_path = Some(PathBuf::from("lobby-lifecycle.ogg"));
        song.charts[0].music_path = song.music_path.clone();
        let chart = Arc::new(song.charts[0].clone());
        let mut game = game::init_gameplay_runtime(
            Arc::new(song),
            [chart.clone(), chart],
            [data.clone(), data],
            game::GameplayViewport::default(),
            game::GameplaySession {
                play_style: if joined_sides == [true, true] {
                    game::GameplayInputPlayStyle::Versus
                } else {
                    game::GameplayInputPlayStyle::Single
                },
                player_side: match side {
                    PlayerSide::P1 => game::GameplayInputPlayerSide::P1,
                    PlayerSide::P2 => game::GameplayInputPlayerSide::P2,
                },
                joined_sides,
                ..Default::default()
            },
            game::GameplayConfig::default(),
            deadsync_chart::SyncPref::Default,
            Default::default(),
            Default::default(),
            EmptyVisuals,
            |_, _, _, _, _| vec![],
            5,
            1.0,
            [deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0); 2],
            std::array::from_fn(|_| GameplayProfile::from(deadsync_profile::Profile::default())),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            [game::CourseLifeConfig::Bar; 2],
            false,
            [0; 2],
        );
        game.drain_audio_commands().for_each(drop);
        Self {
            game,
            lobby: GameplayLobbySession::new(view, joined_sides, side),
        }
    }

    fn music_starts(&mut self) -> usize {
        self.game
            .drain_audio_commands()
            .filter(|command| matches!(command, game::GameplayAudioCommand::PlayMusic { .. }))
            .count()
    }
}

fn joined_view(screen: &str, ready: bool) -> LobbyRuntimeView {
    LobbyRuntimeView {
        snapshot: Arc::new(Snapshot {
            connection: ConnectionState::Connected,
            joined_lobby: Some(JoinedLobby {
                code: "ROOM".into(),
                players: vec![LobbyPlayer {
                    label: "Remote player".into(),
                    ready,
                    screen_name: screen.into(),
                    judgments: None,
                    score: None,
                    ex_score: None,
                }],
                song_info: None,
            }),
            ..Snapshot::default()
        }),
        ..LobbyRuntimeView::default()
    }
}

fn event(action: VirtualAction, pressed: bool, at: Instant) -> InputEvent {
    InputEvent::new(action, 0, pressed, InputSource::Keyboard, at, 0, at, at)
}

fn ready_updates(effects: &[GameplayLobbyAction]) -> Vec<[bool; 2]> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            GameplayLobbyAction::UpdateMachineStats {
                p1_ready, p2_ready, ..
            } => Some([*p1_ready, *p2_ready]),
            GameplayLobbyAction::Disconnect => None,
        })
        .collect()
}

#[test]
fn lobby_hud_chord_consumes_only_the_completing_edge_for_each_local_player() {
    use GameplayLobbyInput::{PassThrough, ToggleHud};
    use VirtualAction::{
        p1_left, p1_menu_left, p1_menu_right, p1_right, p2_menu_left, p2_menu_right,
    };

    let now = Instant::now();
    let mut lobby = GameplayLobbySession::new(
        joined_view(GAMEPLAY_SCREEN_NAME, true),
        [true, true],
        PlayerSide::P1,
    );
    for (action, pressed, millis, expected) in [
        (p1_left, true, 0, PassThrough),
        (p1_right, true, 0, PassThrough),
        (p1_menu_left, true, 0, PassThrough),
        (p2_menu_right, true, 0, PassThrough),
        (p1_menu_right, true, 75, ToggleHud),
        (p1_menu_right, true, 75, PassThrough),
        (p2_menu_left, true, 76, PassThrough),
        (p1_menu_left, false, 77, PassThrough),
        (p1_menu_left, true, 78, ToggleHud),
        (p2_menu_right, false, 79, PassThrough),
        (p2_menu_right, true, 80, ToggleHud),
    ] {
        let timestamp = now + Duration::from_millis(millis);
        assert_eq!(
            lobby.route_input(&event(action, pressed, timestamp)),
            expected
        );
    }
}

#[test]
fn lobby_hud_chord_respects_fallback_side_waiting_and_disconnect() {
    use GameplayLobbyInput::{Consumed, PassThrough, ToggleHud};
    use VirtualAction::{p1_menu_left, p1_menu_right, p2_menu_left, p2_menu_right};

    let now = Instant::now();
    let mut lobby = GameplayLobbySession::new(
        joined_view(GAMEPLAY_SCREEN_NAME, true),
        [false, false],
        PlayerSide::P2,
    );
    for action in [p1_menu_left, p1_menu_right, p2_menu_left] {
        assert_eq!(lobby.route_input(&event(action, true, now)), PassThrough);
    }

    lobby.runtime_view.reconnect_status_text = Some("Reconnecting".into());
    for action in [p2_menu_right, p2_menu_left] {
        assert_eq!(lobby.route_input(&event(action, true, now)), Consumed);
    }
    lobby.runtime_view.reconnect_status_text = None;
    assert_eq!(
        lobby.route_input(&event(p2_menu_right, true, now)),
        PassThrough
    );
    assert_eq!(
        lobby.route_input(&event(p2_menu_left, true, now)),
        ToggleHud
    );

    lobby.runtime_view = LobbyRuntimeView::default();
    for action in [p2_menu_left, p2_menu_right] {
        assert_eq!(lobby.route_input(&event(action, true, now)), PassThrough);
    }
}

#[test]
fn offline_entry_starts_once_without_lobby_effects_or_input_consumption() {
    let mut host = Host::new(
        LobbyRuntimeView {
            // Reconnect text without a joined lobby must not freeze gameplay.
            reconnect_status_text: Some("Reconnecting".into()),
            ..Default::default()
        },
        [true, false],
        PlayerSide::P1,
    );
    assert!(!uses_live_runtime(&host));
    on_enter(&mut host);
    assert!(host.lobby.music_started());
    assert_eq!(host.lobby.ready_sides(), [true, false]);
    assert_eq!(host.music_starts(), 1);

    let mut effects = Vec::with_capacity(2);
    for _ in 0..3 {
        assert!(prepare_update_at(&mut host, &mut effects, Instant::now()));
        assert!(!handle_input(
            &mut host,
            &event(VirtualAction::p1_start, true, Instant::now()),
        ));
    }
    assert!(effects.is_empty());
    assert_eq!(host.music_starts(), 0);
}

#[test]
fn ready_server_snapshot_still_waits_on_entry_then_emits_pre_and_post_start_stats() {
    let mut host = Host::new(
        joined_view("screengameplay", true),
        [true, false],
        PlayerSide::P1,
    );
    assert!(uses_live_runtime(&host));
    on_enter(&mut host);
    assert!(!host.lobby.music_started());
    assert_eq!(host.lobby.ready_sides(), [false, false]);
    assert_eq!(host.music_starts(), 0);

    let mut effects = Vec::with_capacity(2);
    assert!(prepare_update_at(&mut host, &mut effects, Instant::now()));
    assert_eq!(ready_updates(&effects), [[false, false], [true, false]]);
    assert_eq!(host.music_starts(), 1);
    assert!(host.lobby.music_started());
    assert!(host.lobby.local_players_ready());

    effects.clear();
    assert!(prepare_update_at(&mut host, &mut effects, Instant::now()));
    assert_eq!(ready_updates(&effects), [[true, false]]);
    assert_eq!(host.music_starts(), 0);
}

#[test]
fn local_ready_inputs_wait_for_the_server_and_do_not_reach_gameplay() {
    let mut host = Host::new(
        joined_view("ScreenGameplay", false),
        [true, true],
        PlayerSide::P1,
    );
    on_enter(&mut host);
    let now = Instant::now();
    for action in [
        VirtualAction::p1_left,
        VirtualAction::p1_back,
        VirtualAction::p1_start,
        VirtualAction::p2_start,
    ] {
        assert!(handle_input(&mut host, &event(action, true, now)));
    }
    assert_eq!(host.lobby.ready_sides(), [true, true]);
    assert!(host.lobby.local_players_ready());
    assert!(host.game.pending_input_is_empty());
    assert_eq!(host.game.participant_display_itg_score_percent(0), 0.0);

    let mut effects = Vec::with_capacity(2);
    assert!(!prepare_update_at(&mut host, &mut effects, now));
    assert_eq!(ready_updates(&effects), [[true, true]]);
    assert_eq!(host.music_starts(), 0);

    // Readiness alone is insufficient while someone is still on another screen.
    sync_runtime_view(&mut host, joined_view("ScreenSelectMusic", true));
    effects.clear();
    assert!(!prepare_update_at(&mut host, &mut effects, now));
    assert_eq!(host.music_starts(), 0);

    sync_runtime_view(&mut host, joined_view("ScreenGameplay", true));
    effects.clear();
    assert!(prepare_update_at(&mut host, &mut effects, now));
    assert_eq!(effects.len(), 2);
    assert_eq!(host.music_starts(), 1);
    assert_eq!(host.lobby.disconnect_hold_elapsed_at(now), None);
}

#[test]
fn local_side_fallback_ignores_inactive_start_and_routes_p2_stats() {
    let now = Instant::now();
    let mut host = Host::new(
        joined_view("ScreenGameplay", false),
        [false, false],
        PlayerSide::P2,
    );
    on_enter(&mut host);
    assert!(handle_input(
        &mut host,
        &event(VirtualAction::p1_start, true, now),
    ));
    assert_eq!(host.lobby.ready_sides(), [false, false]);
    assert_eq!(host.lobby.disconnect_hold_elapsed_at(now), None);
    assert!(handle_input(
        &mut host,
        &event(VirtualAction::p2_start, true, now),
    ));
    assert_eq!(host.lobby.ready_sides(), [false, true]);
    assert!(host.lobby.local_players_ready());

    let mut effects = Vec::with_capacity(2);
    assert!(!prepare_update_at(&mut host, &mut effects, now));
    let [
        GameplayLobbyAction::UpdateMachineStats {
            p1_ready,
            p2_ready,
            p1_stats,
            p2_stats,
        },
    ] = effects.as_slice()
    else {
        panic!("waiting must publish one machine state");
    };
    assert!(!p1_ready && *p2_ready);
    assert!(p1_stats.is_none());
    let judgments = p2_stats.as_ref().unwrap().judgments.as_ref().unwrap();
    assert_eq!(judgments.total_steps, 4);
}

#[test]
fn releasing_start_cancels_disconnect_hold_without_clearing_readiness() {
    let mut host = Host::new(
        joined_view("ScreenGameplay", false),
        [true, false],
        PlayerSide::P1,
    );
    on_enter(&mut host);
    let now = Instant::now();
    assert!(handle_input(
        &mut host,
        &event(VirtualAction::p1_start, true, now),
    ));
    assert!(handle_input(
        &mut host,
        &event(
            VirtualAction::p1_start,
            false,
            now + Duration::from_millis(4_999),
        ),
    ));
    assert_eq!(host.lobby.ready_sides(), [true, false]);
    assert_eq!(host.lobby.disconnect_hold_elapsed_at(now), None);

    let mut effects = Vec::with_capacity(2);
    assert!(!prepare_update_at(
        &mut host,
        &mut effects,
        now + Duration::from_secs(6),
    ));
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, GameplayLobbyAction::Disconnect))
    );
    assert_eq!(host.music_starts(), 0);
}

#[test]
fn earliest_active_hold_disconnects_at_the_deadline_and_starts_on_the_same_frame() {
    let mut view = joined_view("ScreenGameplay", false);
    view.reconnect_status_text = Some("Reconnecting".into());
    let shared_snapshot = view.snapshot.clone();
    let mut host = Host::new(view, [true, true], PlayerSide::P1);
    on_enter(&mut host);
    let now = Instant::now();
    handle_input(&mut host, &event(VirtualAction::p1_start, true, now));
    handle_input(
        &mut host,
        &event(VirtualAction::p2_start, true, now + Duration::from_secs(2)),
    );
    let mut effects = Vec::with_capacity(2);
    assert!(!prepare_update_at(
        &mut host,
        &mut effects,
        now + Duration::from_millis(4_999),
    ));
    assert_eq!(host.music_starts(), 0);
    effects.clear();

    assert!(prepare_update_at(
        &mut host,
        &mut effects,
        now + Duration::from_secs(5),
    ));
    assert!(matches!(
        effects.as_slice(),
        [GameplayLobbyAction::Disconnect]
    ));
    assert_eq!(host.music_starts(), 1);
    assert!(host.lobby.music_started());
    assert_eq!(host.lobby.ready_sides(), [true, true]);
    assert_eq!(
        host.lobby
            .disconnect_hold_elapsed_at(now + Duration::from_secs(5)),
        None
    );
    assert!(host.lobby.runtime_view().snapshot.joined_lobby.is_none());
    assert_eq!(
        host.lobby.runtime_view().snapshot.connection,
        ConnectionState::Disconnected
    );
    assert!(host.lobby.runtime_view().reconnect_status_text.is_none());
    assert!(
        shared_snapshot.joined_lobby.is_some(),
        "disconnect must preserve other snapshot readers"
    );
    assert!(
        uses_live_runtime(&host),
        "the entry-time refresh policy remains fixed"
    );

    effects.clear();
    assert!(prepare_update_at(
        &mut host,
        &mut effects,
        now + Duration::from_secs(10),
    ));
    assert!(effects.is_empty());
    assert_eq!(host.music_starts(), 0);
}

#[test]
fn reconnect_blocks_joined_start_but_later_snapshot_changes_do_not_pause_music() {
    let mut view = joined_view("ScreenGameplay", true);
    view.reconnect_status_text = Some("Reconnecting".into());
    let mut host = Host::new(view, [true, false], PlayerSide::P1);
    on_enter(&mut host);
    let mut effects = Vec::with_capacity(2);
    let now = Instant::now();
    assert!(!prepare_update_at(&mut host, &mut effects, now));
    assert_eq!(host.music_starts(), 0);

    sync_runtime_view(&mut host, joined_view("ScreenGameplay", true));
    effects.clear();
    assert!(prepare_update_at(&mut host, &mut effects, now));
    assert_eq!(host.music_starts(), 1);

    let mut changed = joined_view("ScreenSelectMusic", false);
    changed.reconnect_status_text = Some("Reconnecting".into());
    sync_runtime_view(&mut host, changed);
    effects.clear();
    assert!(prepare_update_at(&mut host, &mut effects, now));
    assert_eq!(effects.len(), 1);
    assert!(!host.lobby.waiting());
    assert_eq!(host.music_starts(), 0);
}

#[test]
fn connection_state_gates_stats_without_overriding_server_readiness() {
    let mut view = joined_view("ScreenGameplay", true);
    Arc::make_mut(&mut view.snapshot).connection = ConnectionState::Disconnected;
    let mut host = Host::new(view, [true, false], PlayerSide::P1);
    on_enter(&mut host);
    let mut effects = Vec::with_capacity(2);
    assert!(prepare_update_at(&mut host, &mut effects, Instant::now()));
    assert!(effects.is_empty());
    assert_eq!(host.music_starts(), 1);
}
