use super::*;
use crate::perf::{assert_churn_budget, assert_no_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/ui_text/lobby_baseline.rs"
    ));
}

fn joined(count: usize) -> lobbies::JoinedLobby {
    lobbies::JoinedLobby {
        code: "ABCD".to_owned(),
        players: (0..count)
            .map(|index| lobbies::LobbyPlayer {
                label: format!("Player {index}"),
                ready: index % 2 == 0,
                screen_name: "ScreenGameplay".to_owned(),
                judgments: None,
                score: Some(90.0 + (index * 7 % 10) as f32),
                ex_score: Some(88.125),
            })
            .collect(),
        song_info: Some(lobbies::LobbySongInfo {
            song_path: "Pack/Song".to_owned(),
            ..Default::default()
        }),
    }
}

fn params(joined: &lobbies::JoinedLobby) -> CachedRenderParams<'_> {
    CachedRenderParams {
        screen_name: "ScreenGameplay",
        joined,
        z: 995,
        show_song_info: true,
        status_text: Some("Waiting for players"),
        joined_sides: [true, false],
        player_side: PlayerSide::P1,
    }
}

#[test]
fn direct_lobby_text_preserves_known_layout() {
    let empty = joined(0);
    assert_eq!(
        build_body_text(&empty, "ScreenGameplay", true, None),
        "Lobby Code: ABCD\n\nWaiting for players..."
    );
    let mut lobby = joined(2);
    lobby.players[0].label = "Local".to_owned();
    lobby.players[1].label = "Remote".to_owned();
    lobby.players[1].screen_name = "ScreenSelectMusic".to_owned();
    assert_eq!(
        build_body_text(&lobby, "ScreenGameplay", true, Some("Ready?")),
        "Lobby Code: ABCD\n\nReady?\n\n1. Local [\u{2714}]\n    90.00% - 88.12% EX\n\n2. Remote [\u{274c}] - in SelectMusic\n\nPack: Pack\nSong: Song"
    );
}

#[test]
fn direct_lobby_text_only_allocates_output_and_player_order() {
    let lobby = joined(2);
    assert_churn_budget(2, 512, || {
        let text = build_body_text(&lobby, "ScreenGameplay", true, Some("Ready"));
        assert!(text.contains("Pack: Pack\nSong: Song"));
    });
}

#[test]
fn direct_lobby_text_matches_parent_for_order_unicode_and_scores() {
    let scores = [
        None,
        Some(-0.0),
        Some(0.0),
        Some(-7.0),
        Some(98.765),
        Some(f32::NAN),
        Some(f32::INFINITY),
        Some(f32::NEG_INFINITY),
        Some(f32::MAX),
    ];
    let screens = [
        "ScreenGameplay",
        "sCrEeNgAmEpLaY",
        "ScreenEvaluationStage",
        "ScreenSelectMusic",
        "NoScreen",
        "",
        " ScreenOptions ",
        "\u{65e5}\u{672c}",
    ];
    for count in [0, 1, 2, 8, 33, 128] {
        for seed in 0..12 {
            let mut lobby = joined(count);
            for (index, player) in lobby.players.iter_mut().enumerate() {
                player.screen_name = screens[(index + seed) % screens.len()].to_owned();
                player.score = scores[(index + seed) % scores.len()];
                player.ex_score = scores[(index * 5 + seed) % scores.len()];
                player.ready = (index + seed) % 3 == 0;
                player.label = "\u{65e5}\u{1f3b5}e\u{301}".repeat((index + seed) % 12);
            }
            lobby.song_info.as_mut().unwrap().song_path =
                ["", "No slash", "/", "a/b/c", "\u{65e5}\u{672c}/\u{1f3b5}"][seed % 5]
                    .repeat(seed + 1);
            for screen in screens {
                for status in [
                    None,
                    Some(""),
                    Some("\n"),
                    Some("status\r\n\nnext\n"),
                    Some(
                        "\u{1f3b5} This status line is longer than forty-four characters and must be truncated.",
                    ),
                ] {
                    for show_song in [false, true] {
                        assert_eq!(
                            build_body_text(&lobby, screen, show_song, status),
                            baseline::build_body_text(&lobby, screen, show_song, status)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn snapshot_refresh_reuses_names_and_player_storage_without_churn() {
    let mut lobby = joined(8);
    let mut snapshot = LobbyHudSnapshot::from_params(&params(&lobby));
    let players = snapshot.players.as_ptr();
    let names: Vec<_> = snapshot.players.iter().map(|p| p.label.as_ptr()).collect();
    for step in 0..32 {
        for player in &mut lobby.players {
            player.score = Some(step as f32);
            player.ex_score = Some(step as f32 * 0.5);
            player.ready = !player.ready;
        }
        let params = params(&lobby);
        assert!(!snapshot.matches(&params));
        assert_no_churn(|| snapshot.update(&params));
        assert!(snapshot.matches(&params));
        assert_eq!(snapshot.players.as_ptr(), players);
        assert!(
            snapshot
                .players
                .iter()
                .zip(&names)
                .all(|(p, &name)| p.label.as_ptr() == name)
        );
    }
}

#[test]
fn snapshot_refresh_preserves_invalidation_after_membership_and_text_changes() {
    let mut lobby = joined(4);
    let mut actual = LobbyHudSnapshot::from_params(&params(&lobby));
    let mut old = baseline::LobbyHudSnapshot::from_params(&params(&lobby));
    for step in 0..48 {
        match step % 12 {
            0 => lobby.players.reverse(),
            1 => lobby.players[0].label = "new longer player name".repeat(step + 1),
            2 => lobby.players[0].label = "P".to_owned(),
            3 => {
                lobby.players.pop();
            }
            4 => lobby.players.extend(joined(2).players),
            5 => lobby.song_info = None,
            6 => lobby.song_info = joined(0).song_info,
            7 => lobby.code.push('X'),
            8 => lobby.players[0].score = Some(f32::NAN),
            9 => lobby.players[0].score = Some(f32::INFINITY),
            10 => lobby.players[0].screen_name = "ScreenEvaluationStage".to_owned(),
            _ => lobby.players[0].ready = !lobby.players[0].ready,
        }
        let mut params = params(&lobby);
        params.show_song_info = step % 3 != 0;
        params.status_text = [None, Some(""), Some("Short"), Some("A longer status")][step % 4];
        params.screen_name = if step % 2 == 0 {
            "ScreenGameplay"
        } else {
            "ScreenSelectMusic"
        };
        params.joined_sides = [step % 2 == 0, step % 3 == 0];
        params.player_side = if step % 2 == 0 {
            PlayerSide::P1
        } else {
            PlayerSide::P2
        };
        params.z = step as i16;
        assert_eq!(actual.matches(&params), old.matches(&params));
        actual.update(&params);
        old = baseline::LobbyHudSnapshot::from_params(&params);
        // Debug output compares every field, including exact Option states.
        assert_eq!(format!("{actual:?}"), format!("{old:?}"));
        assert!(actual.matches(&params));
        assert!(old.matches(&params));
    }
}

#[test]
fn cache_hits_and_refreshes_still_show_current_text() {
    let mut lobby = joined(4);
    let mut cache = LobbyHudCache::default();
    for step in 0..16 {
        lobby.players[0].score = Some(step as f32);
        let params = params(&lobby);
        let panel = cache.panel(&params);
        let Actor::Text { content, .. } = &panel[1] else {
            panic!("expected text")
        };
        assert_eq!(
            content.as_str(),
            baseline::build_body_text(&lobby, params.screen_name, true, params.status_text)
        );
        let mut cached = None;
        assert_no_churn(|| cached = Some(cache.panel(&params)));
        assert!(Arc::ptr_eq(&panel, &cached.unwrap()));
    }
    assert_eq!(
        cache.stats(),
        LobbyHudCacheStats {
            hits: 16,
            misses: 16
        }
    );
}

#[test]
#[ignore = "manual old/new CPU-cycle and allocation benchmark"]
fn ui_text_lobby_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for count in [0, 2, 8, 32, 128] {
        let lobby = joined(count);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("lobby_text_{count}/{}", if old { "old" } else { "new" });
            if old {
                measure_sampled(&label, 512, count.max(1), || {
                    baseline::build_body_text(
                        black_box(&lobby),
                        "ScreenGameplay",
                        true,
                        Some("Ready"),
                    )
                });
            } else {
                measure_sampled(&label, 512, count.max(1), || {
                    build_body_text(black_box(&lobby), "ScreenGameplay", true, Some("Ready"))
                });
            }
        }
        let mut updated = lobby.clone();
        for player in &mut updated.players {
            player.score = Some(99.123);
            player.ready = !player.ready;
        }
        let states = [params(&lobby), params(&updated)];
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("lobby_snapshot_{count}/{}", if old { "old" } else { "new" });
            let mut index = 0;
            if old {
                let mut snapshot = baseline::LobbyHudSnapshot::from_params(&states[0]);
                measure_sampled(&label, 2048, count.max(1), || {
                    index ^= 1;
                    snapshot = baseline::LobbyHudSnapshot::from_params(black_box(&states[index]));
                    black_box(&snapshot);
                });
            } else {
                let mut snapshot = LobbyHudSnapshot::from_params(&states[0]);
                measure_sampled(&label, 2048, count.max(1), || {
                    index ^= 1;
                    snapshot.update(black_box(&states[index]));
                    black_box(&snapshot);
                });
            }
        }
    }
    // Controls for updates that really change owned text or player-list length.
    let lobby = joined(8);
    let mut renamed = lobby.clone();
    for player in &mut renamed.players {
        player.label = "A renamed player".to_owned();
    }
    let larger = joined(9);
    for (name, updated) in [("renamed", &renamed), ("membership", &larger)] {
        let states = [params(&lobby), params(updated)];
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("lobby_snapshot_{name}/{}", if old { "old" } else { "new" });
            let mut index = 0;
            if old {
                let mut snapshot = baseline::LobbyHudSnapshot::from_params(&states[0]);
                measure_sampled(&label, 2048, 1, || {
                    index ^= 1;
                    snapshot = baseline::LobbyHudSnapshot::from_params(black_box(&states[index]));
                    black_box(&snapshot);
                });
            } else {
                let mut snapshot = LobbyHudSnapshot::from_params(&states[0]);
                measure_sampled(&label, 2048, 1, || {
                    index ^= 1;
                    snapshot.update(black_box(&states[index]));
                    black_box(&snapshot);
                });
            }
        }
    }
}
