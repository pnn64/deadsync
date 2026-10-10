use super::*;
use std::hint::black_box;

// Frozen from committed main 466815487; only internal reference names change.
fn original_ordered_players(joined: &lobbies::JoinedLobby) -> Vec<(usize, &lobbies::LobbyPlayer)> {
    let mut score_players: Vec<_> = joined
        .players
        .iter()
        .enumerate()
        .filter(|(_, player)| is_score_screen(player.screen_name.as_str()))
        .collect();
    score_players.sort_by(|(a_idx, a), (b_idx, b)| {
        match (
            a.score.filter(|score| score.is_finite()),
            b.score.filter(|score| score.is_finite()),
        ) {
            (Some(a_score), Some(b_score)) => {
                b_score.total_cmp(&a_score).then_with(|| a_idx.cmp(b_idx))
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a_idx.cmp(b_idx),
        }
    });

    let mut ordered = score_players;
    ordered.extend(
        joined
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| !is_score_screen(player.screen_name.as_str())),
    );
    ordered
}

fn original_build_body_text(
    joined: &lobbies::JoinedLobby,
    current_screen_name: &str,
    show_song_info: bool,
    status_text: Option<&str>,
) -> String {
    // Reserve typical player/status lines; longer screen names can still grow.
    let mut out = String::with_capacity(
        joined.code.len()
            + 32
            + joined.players.len() * 96
            + status_text.map_or(0, |text| text.len().min(256)),
    );
    write!(out, "Lobby Code: {}\n\n", joined.code).expect("writing to a String cannot fail");
    if let Some(status_text) = status_text {
        for line in status_text.lines() {
            writeln!(out, "{}", Truncated(line, 44)).expect("writing to a String cannot fail");
        }
        out.push('\n');
    }
    let ordered_players = original_ordered_players(joined);
    if ordered_players.is_empty() {
        out.push_str("Waiting for players...");
        return out;
    }
    let show_ready_icons = current_screen_name.eq_ignore_ascii_case("ScreenGameplay")
        && !joined.players.is_empty()
        && !joined.players.iter().all(gameplay_player_ready);
    for (display_index, (_, player)) in ordered_players.into_iter().enumerate() {
        if display_index > 0 {
            out.push_str("\n\n");
        }
        write!(
            out,
            "{}. {}",
            display_index + 1,
            Truncated(&player.label, 22)
        )
        .expect("writing to a String cannot fail");
        if show_ready_icons {
            out.push_str(if gameplay_player_ready(player) {
                " [\u{2714}]"
            } else {
                " [\u{274c}]"
            });
        }
        if !player.screen_name.eq_ignore_ascii_case(current_screen_name) {
            out.push_str(" - in ");
            out.push_str(display_screen_name(&player.screen_name));
        }
        if is_score_screen(&player.screen_name) {
            write!(
                out,
                "\n    {:.2}% - {:.2}% EX",
                percent_value(player.score),
                percent_value(player.ex_score)
            )
            .expect("writing to a String cannot fail");
        }
    }
    if show_song_info && let Some(song_info) = joined.song_info.as_ref() {
        let (pack, song) = song_info
            .song_path
            .split_once('/')
            .unwrap_or(("Unknown", &song_info.song_path));
        write!(
            out,
            "\n\nPack: {}\nSong: {}",
            Truncated(pack, 30),
            Truncated(song, 30)
        )
        .expect("writing to a String cannot fail");
    }
    out
}

fn fixture(count: usize, mode: usize) -> lobbies::JoinedLobby {
    lobbies::JoinedLobby {
        code: "ABCD".into(),
        song_info: None,
        players: (0..count)
            .map(|i| lobbies::LobbyPlayer {
                label: format!("Player {i} é界🎵"),
                ready: i % 2 == 0,
                screen_name: if mode == 0 || (mode == 2 && i % 3 == 0) {
                    "ScreenSelectMusic"
                } else if mode == 1 || i % 2 == 0 {
                    "ScreenGameplay"
                } else {
                    "ScreenEvaluationStage"
                }
                .into(),
                score: [
                    Some(97.5),
                    None,
                    Some(f32::NAN),
                    Some(-0.0),
                    Some(0.0),
                    Some(f32::INFINITY),
                    Some(97.5),
                    Some(-5.0),
                ][i % 8],
                ex_score: Some(99.9),
                judgments: None,
            })
            .collect(),
    }
}

#[test]
fn lobby_order_preserves_scores_ties_and_invalid_values() {
    for count in [0, 1, 2, 8, 9, 32, 256] {
        for mode in 0..3 {
            let mut joined = fixture(count, mode);
            // Mixed gameplay/evaluation and non-score screens exercise the streamed tail.
            if mode == 2 {
                for i in (0..count).step_by(3) {
                    joined.players[i].screen_name = "sCreenSelectMusic".into();
                }
            }
            for screen in [
                "ScreenGameplay",
                "ScreenSelectMusic",
                "ScreenEvaluationStage",
            ] {
                for status in [None, Some("Waiting\nReady é界🎵")] {
                    assert_eq!(
                        build_body_text(&joined, screen, true, status),
                        original_build_body_text(&joined, screen, true, status)
                    );
                }
            }
        }
    }
}

#[test]
fn lobby_body_only_allocates_its_output_for_typical_and_non_score_lobbies() {
    for (count, mode) in [(0, 0), (8, 0), (8, 1), (256, 0)] {
        let joined = fixture(count, mode);
        let (actual, counts) =
            crate::perf::measure(|| build_body_text(&joined, "ScreenGameplay", false, None));
        assert_eq!(
            actual,
            original_build_body_text(&joined, "ScreenGameplay", false, None)
        );
        assert_eq!(counts.allocs, 1, "only the final text buffer is allocated");
        if count > 0 {
            assert_eq!(counts.reallocs, 0);
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_runtime_traversal_lobby_order() {
    for (name, count, mode) in [
        ("empty", 0, 0),
        ("browse8", 8, 0),
        ("score2", 2, 1),
        ("score8", 8, 1),
        ("mixed8", 8, 2),
        ("score32", 32, 1),
        ("mixed32", 32, 2),
        ("browse256", 256, 0),
    ] {
        let joined = fixture(count, mode);
        crate::traversal_perf::compare(
            &format!("hud/{name}"),
            || {
                black_box(original_build_body_text(
                    black_box(&joined),
                    "ScreenGameplay",
                    false,
                    None,
                ));
            },
            || {
                black_box(build_body_text(
                    black_box(&joined),
                    "ScreenGameplay",
                    false,
                    None,
                ));
            },
        );
    }
}
