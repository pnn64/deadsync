use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/lobby_outbound/baseline.rs"
    ));
}

fn stats() -> MachinePlayerStats {
    MachinePlayerStats {
        judgments: Some(LobbyJudgments {
            fantastic_plus: 11,
            fantastics: 22,
            excellents: 33,
            greats: 44,
            decents: 55,
            way_offs: 66,
            misses: 77,
            total_steps: 1234,
            mines_hit: 3,
            total_mines: 12,
            holds_held: 27,
            total_holds: 30,
            rolls_held: 9,
            total_rolls: 10,
        }),
        score: Some(98.12345),
        ex_score: Some(99.23456),
    }
}

fn state(count: usize, name: &str, stats: Option<&MachinePlayerStats>, old: bool) -> Value {
    if old {
        baseline::lobby_machine_state_value(
            (count > 0)
                .then(|| baseline::lobby_machine_player("P1", name, "ScreenGameplay", true, stats)),
            (count > 1).then(|| {
                baseline::lobby_machine_player("P2", "[ds] Bob", "ScreenEvaluation", false, stats)
            }),
        )
    } else {
        lobby_machine_state_value(
            (count > 0).then(|| lobby_machine_player("P1", name, "ScreenGameplay", true, stats)),
            (count > 1)
                .then(|| lobby_machine_player("P2", "[ds] Bob", "ScreenEvaluation", false, stats)),
        )
    }
}

fn song(kind: &str) -> LobbySongInfo {
    match kind {
        "empty" => LobbySongInfo::default(),
        "nonfinite" => LobbySongInfo {
            song_path: "Songs/Pack/Test".into(),
            rate: Some(f32::NAN),
            song_length_seconds: Some(f32::INFINITY),
            ..Default::default()
        },
        _ => {
            let text = if kind == "long" {
                "雪 \"line\"\n\\".repeat(128)
            } else {
                "Snow 雪 \"song\"\n\\".into()
            };
            LobbySongInfo {
                song_path: format!("Songs/Pack/{text}"),
                title: Some(text.clone()),
                artist: Some(text),
                song_length_seconds: Some(123.456),
                chart_hash: Some("0123456789abcdef".into()),
                chart_type: Some("dance-single".into()),
                chart_label: Some("Challenge 14".into()),
                rate: Some(1.1),
            }
        }
    }
}

#[test]
fn command_wrappers_preserve_exact_json_bytes_and_borrow_inputs() {
    let stats = stats();
    let mut values = vec![
        Value::Null,
        Value::Bool(false),
        serde_json::json!({}),
        serde_json::json!([1, "雪", null]),
    ];
    for count in 0..=2 {
        values.push(state(count, "雪 \"A\"\n", Some(&stats), false));
    }
    values.push(serde_json::json!({"nested": [1, {"escaped\nkey": "value\u{0}\\雪"}], "score": -0.0, "u64": u64::MAX}));
    for machine in values {
        let unchanged = machine.clone();
        for code in ["", "ABCD", "\"\\\n雪\u{0}"] {
            for password in ["", "PASS", "\t\"\\é雪"] {
                assert_eq!(
                    baseline::create_lobby_text(&machine, password),
                    create_lobby_text(&machine, password)
                );
                assert_eq!(
                    baseline::join_lobby_text(&machine, code, password),
                    join_lobby_text(&machine, code, password)
                );
            }
        }
        assert_eq!(
            baseline::update_machine_text(&machine),
            update_machine_text(&machine)
        );
        assert_eq!(machine, unchanged);
    }
}

#[test]
fn song_selection_preserves_nulls_escaping_field_order_and_float_numbers() {
    for kind in ["empty", "nonfinite", "full", "long"] {
        let source = song(kind);
        assert_eq!(
            baseline::select_song_text(&source),
            select_song_text(&source),
            "{kind}"
        );
    }
    let mut source = song("full");
    let mut seed = 37u32;
    let extremes = [
        0.0,
        -0.0,
        0.1,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::MAX,
        f32::MIN,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    for value in extremes.into_iter().chain((0..1024).map(|_| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        f32::from_bits(seed)
    })) {
        source.rate = Some(value);
        source.song_length_seconds = Some(-value);
        assert_eq!(
            baseline::select_song_text(&source),
            select_song_text(&source),
            "float bits {:08x}",
            value.to_bits()
        );
    }
    // Any combination of omitted fields must retain explicit JSON nulls.
    for mask in 0..256 {
        let mut s = song("full");
        if mask & 1 != 0 {
            s.artist = None;
        }
        if mask & 2 != 0 {
            s.chart_hash = None;
        }
        if mask & 4 != 0 {
            s.chart_label = None;
        }
        if mask & 8 != 0 {
            s.chart_type = None;
        }
        if mask & 16 != 0 {
            s.rate = None;
        }
        if mask & 32 != 0 {
            s.song_length_seconds = None;
        }
        if mask & 64 != 0 {
            s.song_path.clear();
        }
        if mask & 128 != 0 {
            s.title = None;
        }
        assert_eq!(
            baseline::select_song_text(&s),
            select_song_text(&s),
            "mask {mask}"
        );
    }
}

#[test]
fn owned_machine_values_and_update_pipeline_match_legacy() {
    for name in [
        "",
        " \t\u{2003}",
        "Alice",
        "[DS]",
        " [dS] 雪 ",
        "[D雪]",
        "\"\\\n\u{0}",
    ] {
        for count in 0..=2 {
            for with_stats in [false, true] {
                let mut source = stats();
                for value in [0.0, -0.0, 0.1, 100.0, f32::MAX, f32::INFINITY, f32::NAN] {
                    source.score = Some(value);
                    source.ex_score = Some(-value);
                    let stats = with_stats.then_some(&source);
                    let old = state(count, name, stats, true);
                    let new = state(count, name, stats, false);
                    assert_eq!(old, new);
                    assert_eq!(
                        baseline::update_machine_text(&old),
                        update_machine_text(&new)
                    );
                }
            }
        }
    }
    for absent in 0..8 {
        let mut stats = stats();
        if absent & 1 != 0 {
            stats.judgments = None;
        }
        if absent & 2 != 0 {
            stats.score = None;
        }
        if absent & 4 != 0 {
            stats.ex_score = None;
        }
        assert_eq!(
            state(2, "A", Some(&stats), true),
            state(2, "A", Some(&stats), false)
        );
    }
    for (first, second) in [
        (None, None),
        (Some(""), None),
        (None, Some("P2")),
        (Some("雪"), Some("two")),
    ] {
        let player = |id: &str| lobby_machine_player(id, "A", "", false, None);
        assert_eq!(
            baseline::lobby_machine_state_value(first.map(player), second.map(player)),
            lobby_machine_state_value(first.map(player), second.map(player))
        );
    }
}

#[test]
fn machine_conversion_moves_strings_and_names_need_one_allocation() {
    let p = lobby_machine_player("player-owned", "profile-owned", "screen-owned", true, None);
    let pointers = [
        p.player_id.as_ptr(),
        p.profile_name.as_ptr(),
        p.screen_name.as_ptr(),
    ];
    let value = lobby_machine_state_value(Some(p), None);
    for (key, pointer) in ["playerId", "profileName", "screenName"]
        .into_iter()
        .zip(pointers)
    {
        assert_eq!(value["player1"][key].as_str().unwrap().as_ptr(), pointer);
    }
    for name in [
        "",
        "A",
        "  [ds] Bob  ",
        "[DS]",
        "[D雪]",
        "雪雪雪雪雪",
        "  é\n\" ",
    ] {
        let expected = baseline::lobby_profile_name(name);
        perf::assert_churn_budget(1, expected.len(), || {
            assert_eq!(lobby_profile_name(name), expected)
        });
    }
    let alphabet = [
        '[', 'D', 's', ']', ' ', '\t', '\u{2003}', '雪', 'é', '\u{0}',
    ];
    let mut seed = 17u64;
    for length in 0..128 {
        let name: String = (0..length)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                alphabet[(seed >> 32) as usize % alphabet.len()]
            })
            .collect();
        assert_eq!(
            baseline::lobby_profile_name(&name),
            lobby_profile_name(&name)
        );
    }
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut before = || perf::measure_sampled(&format!("{name}_old"), iterations, units, &mut old);
    let mut after = || perf::measure_sampled(&format!("{name}_new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        after();
        before();
    } else {
        before();
        after();
    }
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation accounting"]
fn lobby_outbound_bench() {
    let stats = stats();
    for count in 0..=2 {
        for with_stats in [false, true] {
            let name = format!("{count}_{}", if with_stats { "stats" } else { "plain" });
            let stats = with_stats.then_some(&stats);
            let machine = state(count, "Alice 雪", stats, false);
            pair(
                &format!("create_{name}"),
                512,
                1,
                || baseline::create_lobby_text(black_box(&machine), black_box("PASS")),
                || create_lobby_text(black_box(&machine), black_box("PASS")),
            );
            pair(
                &format!("join_{name}"),
                512,
                1,
                || {
                    baseline::join_lobby_text(
                        black_box(&machine),
                        black_box("ROOM"),
                        black_box("PASS"),
                    )
                },
                || join_lobby_text(black_box(&machine), black_box("ROOM"), black_box("PASS")),
            );
            pair(
                &format!("update_{name}"),
                512,
                1,
                || baseline::update_machine_text(black_box(&machine)),
                || update_machine_text(black_box(&machine)),
            );
            pair(
                &format!("state_{name}"),
                512,
                1,
                || {
                    state(
                        black_box(count),
                        black_box("Alice 雪"),
                        black_box(stats),
                        true,
                    )
                },
                || {
                    state(
                        black_box(count),
                        black_box("Alice 雪"),
                        black_box(stats),
                        false,
                    )
                },
            );
            pair(
                &format!("pipeline_{name}"),
                512,
                1,
                || {
                    baseline::update_machine_text(&state(
                        black_box(count),
                        black_box("Alice 雪"),
                        black_box(stats),
                        true,
                    ))
                },
                || {
                    update_machine_text(&state(
                        black_box(count),
                        black_box("Alice 雪"),
                        black_box(stats),
                        false,
                    ))
                },
            );
        }
    }
    for kind in ["empty", "full", "nonfinite", "long"] {
        let s = song(kind);
        pair(
            &format!("song_{kind}"),
            512,
            1,
            || baseline::select_song_text(black_box(&s)),
            || select_song_text(black_box(&s)),
        );
    }
    for (label, name) in [
        ("blank", " \t".to_string()),
        ("plain", "Alice".into()),
        ("prefixed", " [ds] Bob ".into()),
        ("unicode", "雪雪雪雪".into()),
        ("long", "雪\\\n".repeat(512)),
    ] {
        pair(
            &format!("name_{label}"),
            1024,
            name.len().max(1),
            || baseline::lobby_profile_name(black_box(&name)),
            || lobby_profile_name(black_box(&name)),
        );
    }
    let long = "雪\\\n".repeat(512);
    pair(
        "state_long",
        256,
        1,
        || {
            state(
                black_box(2),
                black_box(&long),
                Some(black_box(&stats)),
                true,
            )
        },
        || {
            state(
                black_box(2),
                black_box(&long),
                Some(black_box(&stats)),
                false,
            )
        },
    );
}
