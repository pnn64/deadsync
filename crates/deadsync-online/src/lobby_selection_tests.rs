use super::{
    LocalSongSelection, RemoteChartSelection, RemoteSelectionOutcome, RemoteSongOptions,
    SelectMusicPresence, chart_label, confirmed_song_to_publish, local_song_info,
    remote_song_options, resolve_remote_song, song_path,
};
use crate::lobbies::{ConnectionState, JoinedLobby, LobbyPlayer, LobbySongInfo, Snapshot};
use crate::lobby_wait::{LocalLobbyPlayers, SelectMusicLobbySession};
use deadsync_chart::{STANDARD_DIFFICULTY_COUNT, SongData, SongPack, SyncPref};
use deadsync_profile::{PlayStyle, PlayerSide};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const LOCAL_P1: LocalLobbyPlayers = LocalLobbyPlayers {
    joined: [true, false],
    player_side: PlayerSide::P1,
};
const PRESENCE: SelectMusicPresence = SelectMusicPresence {
    screen_name: "ScreenSelectMusic",
    ready: true,
};

fn remote(path: &str) -> LobbySongInfo {
    LobbySongInfo {
        song_path: path.into(),
        title: Some("Remote title".into()),
        artist: Some("Remote artist".into()),
        song_length_seconds: Some(120.0),
        chart_hash: Some("hard".into()),
        chart_type: Some("dance-single".into()),
        chart_label: Some("Hard".into()),
        rate: Some(1.0),
    }
}

fn snapshot(code: &str, song: Option<LobbySongInfo>) -> Snapshot {
    Snapshot {
        connection: ConnectionState::Connected,
        joined_lobby: Some(JoinedLobby {
            code: code.into(),
            players: ["Local", "Remote"]
                .into_iter()
                .map(|label| LobbyPlayer {
                    label: label.into(),
                    ready: false,
                    screen_name: "ScreenSelectMusic".into(),
                    judgments: None,
                    score: None,
                    ex_score: None,
                })
                .collect(),
            song_info: song,
        }),
        ..Snapshot::default()
    }
}

fn record_pending(
    session: &mut SelectMusicLobbySession,
    snapshot: &Snapshot,
    generation: u64,
    outcome: RemoteSelectionOutcome,
) {
    let step = session.prepare_selection_sync(snapshot, generation);
    assert_eq!(step.presence, Some(PRESENCE));
    let selected = step.remote_to_apply.expect("selection should be attempted");
    assert!(std::ptr::eq(
        selected,
        snapshot
            .joined_lobby
            .as_ref()
            .unwrap()
            .song_info
            .as_ref()
            .unwrap()
    ));
    session.record_selection_attempt(selected, generation, outcome);
}

fn retry_for(base: &LobbySongInfo, candidate: &LobbySongInfo) -> bool {
    let mut session = SelectMusicLobbySession::default();
    record_pending(
        &mut session,
        &snapshot("ROOM", Some(base.clone())),
        1,
        RemoteSelectionOutcome::Resolved,
    );
    session
        .prepare_selection_sync(&snapshot("ROOM", Some(candidate.clone())), 1)
        .remote_to_apply
        .is_some()
}

fn publication(local: &LobbySongInfo, remote: &LobbySongInfo) -> Option<LobbySongInfo> {
    confirmed_song_to_publish(&snapshot("ROOM", Some(remote.clone())), LOCAL_P1, || {
        Some(local.clone())
    })
}

fn local_selection() -> LocalSongSelection {
    LocalSongSelection {
        play_style: PlayStyle::Single,
        player_side: PlayerSide::P1,
        music_rate: 1.0,
        selected_steps: [1, 3],
        preferred_steps: [1, 3],
    }
}

fn song(path: &str, charts: &[(&str, &str, &str)]) -> Arc<SongData> {
    // Parse one small in-memory chart to obtain normal game-owned metadata.
    // Each scenario then varies only the catalog facts relevant to selection.
    let parsed = deadsync_simfile::song::parse_song_bytes(
        b"#TITLE:Original;\n#SUBTITLE:Mix;\n#ARTIST:Artist;\n#BPMS:0=120;\n\
          #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:1;\n\
          #NOTES:\n1000\n0100\n0010\n0001\n;\n",
        "ssc",
        Path::new("lobby-selection.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(vec![], vec![], vec![]),
        |_| 123.5,
    )
    .unwrap();
    let mut song = deadsync_simfile::cache::build_song_meta(parsed, 0.0);
    let template = song.charts[0].clone();
    song.simfile_path = PathBuf::from(path);
    song.title = "Original".into();
    song.subtitle = "Mix".into();
    song.translit_title = "Translated".into();
    song.artist = "Artist".into();
    song.music_length_seconds = 123.5;
    song.charts = charts
        .iter()
        .map(|&(chart_type, difficulty, hash)| {
            let mut chart = template.clone();
            chart.chart_type = chart_type.into();
            chart.difficulty = difficulty.into();
            chart.short_hash = hash.into();
            chart
        })
        .collect();
    Arc::new(song)
}

fn pack(songs: Vec<Arc<SongData>>) -> SongPack {
    SongPack {
        group_name: "Library".into(),
        name: "Library".into(),
        sort_title: "Library".into(),
        translit_title: String::new(),
        series: String::new(),
        folder_series: String::new(),
        year: 0,
        sync_pref: SyncPref::Default,
        directory: PathBuf::from("Songs"),
        banner_path: None,
        songs,
    }
}

#[test]
fn connected_joined_sync_always_publishes_presence_even_without_remote_work() {
    let mut session = SelectMusicLobbySession::default();
    let absent_snapshot = Snapshot {
        connection: ConnectionState::Connected,
        ..Snapshot::default()
    };
    let absent = session.prepare_selection_sync(&absent_snapshot, 1);
    assert_eq!(absent.presence, None);
    assert!(absent.remote_to_apply.is_none());

    for connection in [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Error("offline".into()),
    ] {
        let mut runtime = snapshot("ROOM", Some(remote("Pack/Song")));
        runtime.connection = connection;
        let step = session.prepare_selection_sync(&runtime, 1);
        assert_eq!(step.presence, None);
        assert!(step.remote_to_apply.is_none());
    }

    let mut runtime = snapshot("ROOM", None);
    runtime.joined_lobby.as_mut().unwrap().players.clear();
    for _ in 0..3 {
        let step = session.prepare_selection_sync(&runtime, 1);
        assert_eq!(step.presence, Some(PRESENCE));
        assert!(step.remote_to_apply.is_none());
    }
    runtime.joined_lobby.as_mut().unwrap().song_info = Some(remote("Pack/Song"));
    let step = session.prepare_selection_sync(&runtime, 1);
    assert_eq!(step.presence, Some(PRESENCE));
    assert_eq!(
        step.remote_to_apply,
        runtime.joined_lobby.as_ref().unwrap().song_info.as_ref()
    );
}

#[test]
fn stable_success_reuses_normalized_identity_without_allocations_or_generation_retry() {
    let mut session = SelectMusicLobbySession::default();
    let original = snapshot("ROOM", Some(remote("Songs/Pack/Song")));
    record_pending(&mut session, &original, 1, RemoteSelectionOutcome::Resolved);
    let mut equivalent = original.clone();
    let selected = equivalent
        .joined_lobby
        .as_mut()
        .unwrap()
        .song_info
        .as_mut()
        .unwrap();
    selected.song_path = " /Songs\\Pack//Song/ ".into();
    selected.title = Some("Changed display title".into());
    selected.artist = None;
    selected.chart_label = Some("Another label".into());
    selected.song_length_seconds = Some(300.0);
    crate::perf::assert_no_churn(|| {
        for generation in [1, 2, u64::MAX, 0] {
            for _ in 0..16 {
                let step = session.prepare_selection_sync(&equivalent, generation);
                assert_eq!(step.presence, Some(PRESENCE));
                assert!(step.remote_to_apply.is_none());
            }
        }
    });
}

#[test]
fn stable_failure_is_allocation_free_and_retries_only_when_generation_changes() {
    let mut session = SelectMusicLobbySession::default();
    let missing = snapshot("ROOM", Some(remote("Missing/Song")));
    record_pending(
        &mut session,
        &missing,
        7,
        RemoteSelectionOutcome::Unresolved,
    );
    crate::perf::assert_no_churn(|| {
        for _ in 0..64 {
            let step = session.prepare_selection_sync(&missing, 7);
            assert_eq!(step.presence, Some(PRESENCE));
            assert!(step.remote_to_apply.is_none());
        }
    });
    record_pending(
        &mut session,
        &missing,
        8,
        RemoteSelectionOutcome::Unresolved,
    );
    assert!(
        session
            .prepare_selection_sync(&missing, 8)
            .remote_to_apply
            .is_none()
    );
    record_pending(&mut session, &missing, 9, RemoteSelectionOutcome::Resolved);
    assert!(
        session
            .prepare_selection_sync(&missing, 10)
            .remote_to_apply
            .is_none()
    );
}

#[test]
fn successful_and_failed_selections_remain_independent_until_replaced() {
    let mut session = SelectMusicLobbySession::default();
    let a = snapshot("ROOM", Some(remote("Pack/A")));
    let b = snapshot("ROOM", Some(remote("Pack/B")));
    record_pending(&mut session, &a, 1, RemoteSelectionOutcome::Resolved);
    record_pending(&mut session, &b, 1, RemoteSelectionOutcome::Unresolved);
    for runtime in [&a, &b, &a, &b] {
        assert!(
            session
                .prepare_selection_sync(runtime, 1)
                .remote_to_apply
                .is_none()
        );
    }
    record_pending(&mut session, &b, 2, RemoteSelectionOutcome::Resolved);
    assert!(
        session
            .prepare_selection_sync(&b, 2)
            .remote_to_apply
            .is_none()
    );
    assert!(
        session
            .prepare_selection_sync(&a, 2)
            .remote_to_apply
            .is_some()
    );
}

#[test]
fn successful_key_survives_song_absence_and_disconnect_but_not_leave_or_room_change() {
    let mut session = SelectMusicLobbySession::default();
    let original = snapshot("ROOM", Some(remote("Pack/A")));
    record_pending(&mut session, &original, 1, RemoteSelectionOutcome::Resolved);
    let absent_song = snapshot("ROOM", None);
    assert!(
        session
            .prepare_selection_sync(&absent_song, 1)
            .remote_to_apply
            .is_none()
    );
    assert!(
        session
            .prepare_selection_sync(&original, 1)
            .remote_to_apply
            .is_none()
    );
    let mut disconnected = original.clone();
    disconnected.connection = ConnectionState::Disconnected;
    assert!(
        session
            .prepare_selection_sync(&disconnected, 1)
            .presence
            .is_none()
    );
    assert!(
        session
            .prepare_selection_sync(&original, 1)
            .remote_to_apply
            .is_none()
    );

    let _ = session.prepare_selection_sync(&Snapshot::default(), 1);
    record_pending(&mut session, &original, 1, RemoteSelectionOutcome::Resolved);
    let mut other_room = original.clone();
    other_room.connection = ConnectionState::Disconnected;
    other_room.joined_lobby.as_mut().unwrap().code = "OTHER".into();
    let _ = session.prepare_selection_sync(&other_room, 1);
    other_room.connection = ConnectionState::Connected;
    record_pending(
        &mut session,
        &other_room,
        1,
        RemoteSelectionOutcome::Resolved,
    );
    // Returning to the prior room also starts a new room-local selection history.
    assert!(
        session
            .prepare_selection_sync(&original, 1)
            .remote_to_apply
            .is_some()
    );
}

#[test]
fn failed_key_retries_after_absence_disconnect_or_room_change_at_the_same_generation() {
    let original = snapshot("ROOM", Some(remote("Missing/Song")));
    let mut disconnected = original.clone();
    disconnected.connection = ConnectionState::Disconnected;
    let mut other_room = original.clone();
    other_room.joined_lobby.as_mut().unwrap().code = "OTHER".into();
    for interruption in [
        Snapshot::default(),
        snapshot("ROOM", None),
        disconnected,
        other_room,
    ] {
        let mut session = SelectMusicLobbySession::default();
        record_pending(
            &mut session,
            &original,
            42,
            RemoteSelectionOutcome::Unresolved,
        );
        let _ = session.prepare_selection_sync(&interruption, 42);
        record_pending(
            &mut session,
            &original,
            42,
            RemoteSelectionOutcome::Unresolved,
        );
    }
}

#[test]
fn cached_identity_keeps_case_and_rate_bits_but_defaults_missing_metadata() {
    let base = remote("Pack/Song");
    let mut equivalent = base.clone();
    equivalent.song_path = " /Pack\\Song// ".into();
    assert!(!retry_for(&base, &equivalent));
    equivalent.song_path = "pack/Song".into();
    assert!(retry_for(&base, &equivalent));
    let mut changed = base.clone();
    changed.chart_hash = Some("HARD".into());
    assert!(retry_for(&base, &changed));
    changed = base.clone();
    changed.chart_type = Some("DANCE-SINGLE".into());
    assert!(retry_for(&base, &changed));
    changed.chart_type = Some(" dance-single ".into());
    assert!(retry_for(&base, &changed));
    changed = base.clone();
    changed.rate = Some(f32::from_bits(1.0_f32.to_bits() + 1));
    assert!(retry_for(&base, &changed));
    changed.rate = None;
    assert!(!retry_for(&base, &changed));

    let mut unspecified = base.clone();
    unspecified.chart_hash = None;
    unspecified.chart_type = None;
    let mut empty = unspecified.clone();
    empty.chart_hash = Some(String::new());
    empty.chart_type = Some(String::new());
    assert!(!retry_for(&unspecified, &empty));

    let mut positive_zero = base.clone();
    positive_zero.rate = Some(0.0);
    let mut negative_zero = positive_zero.clone();
    negative_zero.rate = Some(-0.0);
    assert!(retry_for(&positive_zero, &negative_zero));
    let mut nan = base.clone();
    nan.rate = Some(f32::from_bits(0x7fc0_0001));
    assert!(!retry_for(&nan, &nan));
    let mut other_nan = nan.clone();
    other_nan.rate = Some(f32::from_bits(0x7fc0_0002));
    assert!(retry_for(&nan, &other_nan));
}

#[test]
fn confirmation_builds_local_payload_only_for_connected_remote_participants() {
    let connected = snapshot("ROOM", None);
    let mut local_only = connected.clone();
    local_only
        .joined_lobby
        .as_mut()
        .unwrap()
        .players
        .truncate(1);
    let mut disconnected = connected.clone();
    disconnected.connection = ConnectionState::Disconnected;
    let both = LocalLobbyPlayers {
        joined: [true, true],
        player_side: PlayerSide::P1,
    };
    let no_joined_profiles = LocalLobbyPlayers {
        joined: [false, false],
        player_side: PlayerSide::P2,
    };
    for (runtime, local, expected_calls) in [
        (Snapshot::default(), LOCAL_P1, 0),
        (
            Snapshot {
                connection: ConnectionState::Connected,
                ..Snapshot::default()
            },
            LOCAL_P1,
            0,
        ),
        (disconnected, LOCAL_P1, 0),
        (local_only, LOCAL_P1, 0),
        (connected.clone(), both, 0),
        (connected.clone(), LOCAL_P1, 1),
        (connected.clone(), no_joined_profiles, 1),
    ] {
        let calls = Cell::new(0);
        let payload = remote("Pack/Local");
        let published = confirmed_song_to_publish(&runtime, local, || {
            calls.set(calls.get() + 1);
            Some(payload.clone())
        });
        assert_eq!(calls.get(), expected_calls);
        assert_eq!(published, (expected_calls == 1).then_some(payload));
    }
    let calls = Cell::new(0);
    assert_eq!(
        confirmed_song_to_publish(&connected, LOCAL_P1, || {
            calls.set(calls.get() + 1);
            None
        }),
        None
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn confirmation_uses_optional_remote_constraints_and_preserves_the_local_payload() {
    let local = remote("Pack/Song");
    let mut other = local.clone();
    other.song_path = " /Pack\\Song// ".into();
    other.chart_type = Some(" DANCE-SINGLE ".into());
    other.title = Some("Another title".into());
    other.artist = None;
    other.song_length_seconds = Some(999.0);
    other.chart_label = Some("Different label".into());
    assert_eq!(publication(&local, &other), None);

    other.chart_hash = None;
    other.chart_type = None;
    other.rate = None;
    assert_eq!(publication(&local, &other), None);
    other.chart_hash = Some(String::new());
    other.chart_type = Some("  ".into());
    assert_eq!(publication(&local, &other), None);
    other.chart_hash = Some("HARD".into());
    assert_eq!(publication(&local, &other), Some(local.clone()));
    other = local.clone();
    other.song_path = "pack/Song".into();
    assert_eq!(publication(&local, &other), Some(local.clone()));
    other = local.clone();
    other.chart_type = Some("dance-double".into());
    assert_eq!(publication(&local, &other), Some(local.clone()));
    let mut missing_local_type = local.clone();
    missing_local_type.chart_type = None;
    assert_eq!(
        publication(&missing_local_type, &local),
        Some(missing_local_type)
    );
}

#[test]
fn confirmation_ignores_invalid_remote_rates_and_keeps_a_strict_tolerance() {
    let local = remote("Pack/Song");
    for rate in [
        None,
        Some(f32::NAN),
        Some(f32::INFINITY),
        Some(f32::NEG_INFINITY),
        Some(0.0),
        Some(-1.0),
    ] {
        let mut other = local.clone();
        other.rate = rate;
        assert_eq!(publication(&local, &other), None);
    }
    let mut other = local.clone();
    other.rate = Some(1.0004);
    assert_eq!(publication(&local, &other), None);
    other.rate = Some(1.0006);
    assert_eq!(publication(&local, &other), Some(local.clone()));
    let mut no_local_rate = local.clone();
    no_local_rate.rate = None;
    assert_eq!(publication(&no_local_rate, &local), Some(no_local_rate));

    // These exact f32 operands put the difference on the tolerance boundary.
    let mut boundary_local = local.clone();
    boundary_local.rate = Some(0.0);
    other.rate = Some(0.0005);
    assert_eq!(publication(&boundary_local, &other), Some(boundary_local));
}

#[test]
fn resolver_prioritizes_visible_exact_matches_before_ordered_library_matches() {
    let first_visible = song("Songs/Pack/Song/first.ssc", &[]);
    let second_visible = song("OtherRoot/Pack/Song/second.ssc", &[]);
    let library_match = song("Library/Pack/Song/library.ssc", &[]);
    let groups = [first_visible.clone(), second_visible];
    let library = [pack(vec![library_match.clone()])];
    let resolved = resolve_remote_song(" /pAcK\\sOnG// ", groups.iter(), &library).unwrap();
    assert!(Arc::ptr_eq(&resolved, &first_visible));
    let resolved = resolve_remote_song("Songs/Pack/Song", groups.iter(), &library).unwrap();
    assert!(
        Arc::ptr_eq(&resolved, &library_match),
        "only exact matches from the grouped wheel precede catalog lookup"
    );
}

#[test]
fn resolver_defers_first_leaf_fallback_until_all_library_candidates_are_checked() {
    let group_leaf = song("Songs/Visible/Song/song.ssc", &[]);
    let first_leaf = song("Songs/First/Song/song.ssc", &[]);
    let later_leaf = song("Songs/Later/Song/song.ssc", &[]);
    let exact = song("Songs/Target/Song/song.ssc", &[]);
    let invalid_path = song("flat.ssc", &[]);
    let library = [
        pack(vec![invalid_path, first_leaf.clone()]),
        pack(vec![later_leaf, exact.clone()]),
    ];
    let groups = [group_leaf];
    let resolved = resolve_remote_song("Target/Song", groups.iter(), &library).unwrap();
    assert!(Arc::ptr_eq(&resolved, &exact));
    let resolved = resolve_remote_song("Songs/TARGET/SONG", groups.iter(), &library).unwrap();
    assert!(Arc::ptr_eq(&resolved, &exact));
    let resolved = resolve_remote_song("Missing/Song", groups.iter(), &library).unwrap();
    assert!(Arc::ptr_eq(&resolved, &first_leaf));
    assert!(resolve_remote_song("Missing/Absent", groups.iter(), &library).is_none());
}

#[test]
fn local_payload_uses_physical_path_current_chart_and_original_display_metadata() {
    let mut target = song(
        "Songs/Series/Pack/Song/chart.ssc",
        &[
            ("dance-single", "Easy", "easy"),
            ("dance-single", "Hard", "hard"),
        ],
    );
    let mut local = local_selection();
    local.play_style = PlayStyle::Versus;
    local.player_side = PlayerSide::P2;
    local.music_rate = 1.25;
    assert_eq!(song_path(&target).as_deref(), Some("Pack/Song"));
    assert_eq!(
        local_song_info(&target, local),
        Some(LobbySongInfo {
            song_path: "Pack/Song".into(),
            title: Some("Original Mix".into()),
            artist: Some("Artist".into()),
            song_length_seconds: Some(123.5),
            chart_hash: Some("hard".into()),
            chart_type: Some("dance-single".into()),
            chart_label: Some("Hard".into()),
            rate: Some(1.25),
        })
    );

    let chart = &mut Arc::make_mut(&mut target).charts[1];
    chart.difficulty = "eDiT".into();
    chart.description = " authored edit ".into();
    assert_eq!(chart_label(chart), "eDiT ( authored edit )");
    chart.description = " \t ".into();
    assert_eq!(chart_label(chart), "eDiT");
    local.selected_steps[1] = STANDARD_DIFFICULTY_COUNT;
    assert_eq!(
        local_song_info(&target, local)
            .unwrap()
            .chart_label
            .as_deref(),
        Some("eDiT")
    );
    local.play_style = PlayStyle::Double;
    assert!(local_song_info(&target, local).is_none());
    Arc::make_mut(&mut target).simfile_path = PathBuf::from("flat.ssc");
    assert!(song_path(&target).is_none());
    assert!(local_song_info(&target, local_selection()).is_none());
}

#[test]
fn remote_options_keep_local_style_use_hash_then_preference_and_preserve_edit_preference() {
    let target = song(
        "Songs/Pack/Song/chart.ssc",
        &[
            ("dance-single", "Easy", "easy"),
            ("dance-single", "Hard", "hard"),
            ("dance-single", "Edit", "edit"),
            ("dance-double", "Challenge", "double"),
        ],
    );
    let local = local_selection();
    let mut selected = remote("Pack/Song");
    selected.chart_type = Some("dance-double".into());
    assert_eq!(
        remote_song_options(&target, &selected, local, 1.0).chart,
        Some(RemoteChartSelection {
            player_index: 0,
            steps_index: 3,
            preferred_difficulty_index: Some(3),
        })
    );
    selected.chart_hash = Some("double".into());
    assert_eq!(
        remote_song_options(&target, &selected, local, 1.0).chart,
        Some(RemoteChartSelection {
            player_index: 0,
            steps_index: 1,
            preferred_difficulty_index: Some(1),
        })
    );
    selected.chart_hash = Some("edit".into());
    assert_eq!(
        remote_song_options(&target, &selected, local, 1.0).chart,
        Some(RemoteChartSelection {
            player_index: 0,
            steps_index: STANDARD_DIFFICULTY_COUNT,
            preferred_difficulty_index: None,
        })
    );
    selected.chart_hash = None;
    let mut after_focus = local;
    after_focus.preferred_steps[0] = 3;
    assert_eq!(
        remote_song_options(&target, &selected, after_focus, 1.0)
            .chart
            .unwrap()
            .steps_index,
        3
    );
}

#[test]
fn canonical_player_mapping_selects_the_current_side_for_all_play_styles() {
    for (style, expected_p2_index) in [
        (PlayStyle::Single, 0),
        (PlayStyle::Double, 0),
        (PlayStyle::PumpSingle, 0),
        (PlayStyle::PumpDouble, 0),
        (PlayStyle::Versus, 1),
        (PlayStyle::PumpVersus, 1),
        (PlayStyle::Couple, 1),
        (PlayStyle::PumpCouple, 1),
        (PlayStyle::Routine, 1),
        (PlayStyle::PumpRoutine, 1),
    ] {
        let target = song(
            "Songs/Pack/Song/chart.ssc",
            &[
                (style.chart_type(), "Easy", "easy"),
                (style.chart_type(), "Hard", "hard"),
            ],
        );
        let mut selected = remote("Pack/Song");
        selected.chart_hash = None;
        for (side, expected_index, expected_steps, expected_hash) in [
            (PlayerSide::P1, 0, 1, "easy"),
            (
                PlayerSide::P2,
                expected_p2_index,
                if expected_p2_index == 0 { 1 } else { 3 },
                if expected_p2_index == 0 {
                    "easy"
                } else {
                    "hard"
                },
            ),
        ] {
            let mut local = local_selection();
            local.play_style = style;
            local.player_side = side;
            assert_eq!(
                remote_song_options(&target, &selected, local, 1.0).chart,
                Some(RemoteChartSelection {
                    player_index: expected_index,
                    steps_index: expected_steps,
                    preferred_difficulty_index: Some(expected_steps),
                }),
                "style {style:?}, side {side:?}"
            );
            assert_eq!(
                local_song_info(&target, local)
                    .unwrap()
                    .chart_hash
                    .as_deref(),
                Some(expected_hash)
            );
        }
    }
}

#[test]
fn resolved_song_without_a_local_chart_can_apply_rate_and_finish_synchronization() {
    let target = song(
        "Songs/Pack/Song/chart.ssc",
        &[("dance-double", "Hard", "double")],
    );
    let library = [pack(vec![target.clone()])];
    let mut selected = remote("Pack/Song");
    selected.rate = Some(1.5);
    let runtime = snapshot("ROOM", Some(selected.clone()));
    let mut session = SelectMusicLobbySession::default();
    assert!(
        session
            .prepare_selection_sync(&runtime, 1)
            .remote_to_apply
            .is_some()
    );
    let resolved = resolve_remote_song("Pack/Song", std::iter::empty(), &library).unwrap();
    assert!(Arc::ptr_eq(&resolved, &target));
    let options = remote_song_options(&resolved, &selected, local_selection(), 1.0);
    assert_eq!(
        options,
        RemoteSongOptions {
            chart: None,
            music_rate: Some(1.5)
        }
    );
    session.record_selection_attempt(&selected, 1, RemoteSelectionOutcome::Resolved);
    assert!(
        session
            .prepare_selection_sync(&runtime, 2)
            .remote_to_apply
            .is_none()
    );
}

#[test]
fn remote_rates_validate_clamp_and_compare_against_the_pre_focus_rate() {
    let target = song("Songs/Pack/Song/chart.ssc", &[]);
    for (rate, previous, expected) in [
        (None, 1.0, None),
        (Some(f32::NAN), 1.0, None),
        (Some(f32::INFINITY), 1.0, None),
        (Some(0.0), 1.0, None),
        (Some(-1.0), 1.0, None),
        (Some(0.1), 1.0, Some(0.5)),
        (Some(8.0), 1.0, Some(3.0)),
        (Some(0.1), 0.5, None),
        (Some(8.0), 3.0, None),
        (Some(1.0004), 1.0, None),
        (Some(1.0006), 1.0, Some(1.0006)),
        (Some(1.25), 1.0, Some(1.25)),
    ] {
        let mut selected = remote("Pack/Song");
        selected.rate = rate;
        let mut after_focus = local_selection();
        after_focus.music_rate = 1.25;
        assert_eq!(
            remote_song_options(&target, &selected, after_focus, previous).music_rate,
            expected
        );
    }
}

#[test]
fn preview_clear_follows_rate_change_and_exact_canonical_path_difference() {
    let target = song("Songs/Pack/Song/chart.ssc", &[]);
    let unchanged = RemoteSongOptions::default();
    assert!(!unchanged.should_clear_preview(Some("Pack/Song"), &target));
    assert!(unchanged.should_clear_preview(Some("Pack/Other"), &target));
    assert!(unchanged.should_clear_preview(Some("pack/Song"), &target));
    assert!(unchanged.should_clear_preview(None, &target));
    let changed = RemoteSongOptions {
        chart: None,
        music_rate: Some(1.5),
    };
    assert!(changed.should_clear_preview(Some("Pack/Song"), &target));
    crate::perf::assert_no_churn(|| {
        assert!(changed.should_clear_preview(Some("Pack/Song"), &target));
    });
    let invalid = song("flat.ssc", &[]);
    assert!(!unchanged.should_clear_preview(None, &invalid));
    assert!(unchanged.should_clear_preview(Some("Pack/Song"), &invalid));
}
