use deadsync_score::leaderboard::{
    PlayerLeaderboardCacheKey, PlayerLeaderboardCacheState, arrowcloud_pane_kind_from_type,
    format_scorebox_rank, format_scorebox_score_value, queue_player_leaderboard_refresh,
    scorebox_machine_tag,
};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/leaderboard_text_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[test]
fn pane_aliases_preserve_ascii_folding_ignored_characters_and_bounds() {
    let check = |raw: &str| {
        assert_eq!(
            arrowcloud_pane_kind_from_type(raw),
            original::arrowcloud_pane_kind_from_type(raw),
            "{raw:?}"
        )
    };
    for alias in ["itg", "ex", "hex", "hardex"] {
        for mask in 0..(1 << alias.len()) {
            let case: String = alias
                .bytes()
                .enumerate()
                .map(|(i, byte)| {
                    char::from(if mask & (1 << i) == 0 {
                        byte
                    } else {
                        byte.to_ascii_uppercase()
                    })
                })
                .collect();
            check(&case);
            check(&format!("\u{2003}{case}\u{a0}"));
        }
        for index in 0..=alias.len() {
            for byte in 0..=127 {
                check(&format!(
                    "{}{}{}",
                    &alias[..index],
                    char::from(byte),
                    &alias[index..]
                ));
            }
            for extra in ["\u{e9}", "\u{96ea}", "\u{212a}", "\u{ff25}", "-_ .\0"] {
                check(&format!("{}{extra}{}", &alias[..index], &alias[index..]));
            }
        }
        check(&format!("{alias}{}", "x".repeat(4096)));
        check(&format!(
            "{}{alias}{}",
            "\u{96ea}-_".repeat(1024),
            "\u{2003}.".repeat(1024)
        ));
    }
    for raw in [
        "",
        "\u{96ea}",
        "-_.\0",
        "event",
        "hardexx",
        "1ex",
        "\u{ff25}\u{ff38}",
    ] {
        check(raw);
    }
}

#[test]
fn machine_tags_preserve_four_scalar_values_trimming_overrides_and_ascii_case() {
    let check = |tag: Option<&str>, name: &str| {
        assert_eq!(
            scorebox_machine_tag(tag, name),
            original::scorebox_machine_tag(tag, name),
            "{tag:?} / {name:?}"
        )
    };
    let values = [
        "",
        " \r\n\t",
        "a",
        "Abc",
        "abcd5",
        "\u{2003} aBcD \u{a0}",
        "\u{e9}\u{96ea}\u{1f600}\u{df}tail",
        "e\u{301}abz",
        "\0x\u{7f}z",
        "\u{200d}\u{fe0f}abz",
    ];
    for name in values {
        check(None, name);
        for tag in values {
            check(Some(tag), name);
        }
    }
    for byte in 0..=127 {
        for position in 0..=6 {
            let base = "AbCdEf";
            let text = format!(
                "{}{}{}",
                &base[..position],
                char::from(byte),
                &base[position..]
            );
            check(None, &text);
            check(Some(&text), "ignored");
        }
    }
    for codepoint in (0..=0x10ffff).step_by(113) {
        let Some(ch) = char::from_u32(codepoint) else {
            continue;
        };
        for position in 0..=4 {
            let base = "abcd";
            let text = format!("{}{}{}suffix", &base[..position], ch, &base[position..]);
            check(None, &text);
            check(Some(&format!("\u{2003}{text}\u{a0}")), "ignored");
        }
    }
    check(None, &"a".repeat(65536));
    check(None, &"\u{96ea}".repeat(16384));
}

fn keys(long: bool) -> Vec<PlayerLeaderboardCacheKey> {
    (0..64)
        .map(|i| PlayerLeaderboardCacheKey {
            chart_hash: format!("{i:016x}"),
            api_key: format!("g{i:03}{}", "g".repeat(if long { 508 } else { 60 })),
            arrowcloud_api_key: format!("a{i:03}{}", "a".repeat(if long { 508 } else { 60 })),
            include_arrowcloud: i & 1 != 0,
            show_ex_score: i & 2 != 0,
        })
        .collect()
}

fn stored_pointers(
    map: &HashMap<PlayerLeaderboardCacheKey, usize>,
    key: &PlayerLeaderboardCacheKey,
) -> Option<[*const u8; 3]> {
    map.get_key_value(key).map(|(stored, _)| {
        [
            stored.chart_hash.as_ptr(),
            stored.api_key.as_ptr(),
            stored.arrowcloud_api_key.as_ptr(),
        ]
    })
}

#[test]
fn refresh_updates_preserve_all_key_fields_values_capacity_and_stored_keys() {
    let mut keys = keys(false);
    let base = keys[0].clone();
    for index in 0..5 {
        let mut key = base.clone();
        match index {
            0 => key.chart_hash.push('x'),
            1 => key.api_key.push('x'),
            2 => key.arrowcloud_api_key.push('x'),
            3 => key.include_arrowcloud = !key.include_arrowcloud,
            _ => key.show_ex_score = !key.show_ex_score,
        }
        keys.push(key);
    }
    keys.push(PlayerLeaderboardCacheKey {
        chart_hash: String::new(),
        api_key: String::new(),
        arrowcloud_api_key: String::new(),
        include_arrowcloud: false,
        show_ex_score: false,
    });
    for capacity in [0, 1, 64, 256] {
        let mut new = HashMap::with_capacity(capacity);
        let mut old = new.clone();
        let mut state = 0x243f_6a88_85a3_08d3u64;
        for operation in 0..4096 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let key = &keys[state as usize % keys.len()];
            if operation % 257 == 0 {
                new.clear();
                old.clear();
            }
            if operation % 17 == 0 {
                assert_eq!(new.remove(key), old.remove(key));
            }
            let value = [0, 1, 5, 10, 100, usize::MAX, state as usize][operation % 7];
            let pointers = stored_pointers(&new, key);
            queue_player_leaderboard_refresh(&mut new, key, value);
            original::queue_player_leaderboard_refresh(&mut old, key, value);
            assert_eq!(new, old, "operation {operation}");
            assert_eq!(new.capacity(), old.capacity(), "operation {operation}");
            if let Some(pointers) = pointers {
                assert_eq!(stored_pointers(&new, key), Some(pointers));
            }
        }
    }
}

fn pane_inputs(decorated: bool) -> Vec<String> {
    (0..512)
        .map(|i| {
            let name = ["itg", "EX", "HardEX", "hex", "event", "", "\u{96ea}", "???"][i % 8];
            if decorated {
                name.chars().map(|ch| format!("{ch}-\u{96ea}_")).collect()
            } else {
                name.to_owned()
            }
        })
        .collect()
}

fn tag_inputs(kind: usize) -> Vec<(Option<String>, String)> {
    (0..512)
        .map(|i| {
            let name = match kind {
                0 => format!(" user{i:04} "),
                1 => [
                    "\u{e9}\u{96ea}\u{1f600}\u{df}tail",
                    "\u{96ea}\u{96ea}\u{96ea}\u{96ea}tail",
                    "\u{1f600}\u{1f601}\u{1f602}\u{1f603}tail",
                ][i % 3]
                    .to_owned(),
                2 => [
                    "",
                    " \t",
                    "  ab  ",
                    "\u{2003}ab\u{e9}\u{96ea}\u{1f600}",
                    "e\u{301}abcd",
                ][i % 5]
                    .to_owned(),
                _ => format!("user{}", "x".repeat(252)),
            };
            if i % 2 == 0 {
                (Some(name), "ignored".to_owned())
            } else {
                (None, name)
            }
        })
        .collect()
}

#[test]
fn row_text_and_request_state_preserve_composed_results() {
    let panes = pane_inputs(true);
    let tags = tag_inputs(2);
    for (pane, (tag, name)) in panes.iter().zip(&tags) {
        assert_eq!(
            (
                arrowcloud_pane_kind_from_type(pane),
                scorebox_machine_tag(tag.as_deref(), name)
            ),
            (
                original::arrowcloud_pane_kind_from_type(pane),
                original::scorebox_machine_tag(tag.as_deref(), name)
            )
        );
    }
    let key = keys(false).remove(0);
    let mut state = PlayerLeaderboardCacheState::default();
    state.in_flight.insert(key.clone(), 5);
    let now = Instant::now();
    for (count, expected) in [
        (10, 10),
        (20, 20),
        (7, 20),
        (usize::MAX, usize::MAX),
        (30, usize::MAX),
    ] {
        let decision = state.request_leaderboard(&key, count, true, now);
        assert!(!decision.should_spawn);
        assert_eq!(decision.requested_max_entries, count);
        assert!(decision.snapshot.loading);
        assert_eq!(state.in_flight.get(&key), Some(&5));
        assert_eq!(state.pending_refresh.get(&key), Some(&expected));
    }
}

fn measure(label: &str, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, 128, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_leaderboard_text() {
    let panes = pane_inputs(false);
    let decorated = pane_inputs(true);
    let long: Vec<_> = (0..64)
        .map(|i| {
            format!(
                "{}{}",
                ["itg", "ex", "hex", "hardex"][i % 4],
                "x".repeat(256)
            )
        })
        .collect();
    for (label, values) in [
        ("pane/plain", &panes),
        ("pane/decorated", &decorated),
        ("pane/long_rejected", &long),
    ] {
        measure(label, || {
            for raw in black_box(values) {
                black_box(arrowcloud_pane_kind_from_type(black_box(raw)));
            }
        });
    }
    for (label, kind) in [
        ("tags/ascii", 0),
        ("tags/unicode", 1),
        ("tags/mixed", 2),
        ("tags/long_name", 3),
    ] {
        let values = tag_inputs(kind);
        measure(label, || {
            for (tag, name) in black_box(&values) {
                black_box(scorebox_machine_tag(
                    black_box(tag.as_deref()),
                    black_box(name),
                ));
            }
        });
    }
    let keys = keys(false);
    let mut map = HashMap::with_capacity(keys.len());
    for key in &keys {
        map.insert(key.clone(), 0);
    }
    measure("queue/hot", || {
        for i in 0..512 {
            queue_player_leaderboard_refresh(
                black_box(&mut map),
                black_box(&keys[i % keys.len()]),
                black_box(10 + i % 32),
            );
        }
    });
    let empty = PlayerLeaderboardCacheKey {
        chart_hash: String::new(),
        api_key: String::new(),
        arrowcloud_api_key: String::new(),
        include_arrowcloud: false,
        show_ex_score: false,
    };
    let mut empty_map = HashMap::from([(empty.clone(), 0)]);
    measure("queue/empty_key", || {
        for i in 0..512 {
            queue_player_leaderboard_refresh(
                black_box(&mut empty_map),
                black_box(&empty),
                black_box(i),
            );
        }
    });
    let mut cold = HashMap::with_capacity(keys.len());
    measure("queue/cold", || {
        for key in black_box(&keys) {
            queue_player_leaderboard_refresh(black_box(&mut cold), black_box(key), black_box(10));
        }
        black_box(&cold);
        cold.clear();
    });
    let mut mixed = HashMap::with_capacity(keys.len());
    measure("queue/mixed", || {
        for key in black_box(&keys) {
            queue_player_leaderboard_refresh(black_box(&mut mixed), black_box(key), black_box(10));
        }
        for i in 0..512 {
            queue_player_leaderboard_refresh(
                black_box(&mut mixed),
                black_box(&keys[i % keys.len()]),
                black_box(10 + i % 32),
            );
        }
        black_box(&mixed);
        mixed.clear();
    });
    let long_keys = self::keys(true);
    let mut large = HashMap::with_capacity(long_keys.len());
    for key in &long_keys {
        large.insert(key.clone(), 0);
    }
    measure("queue/long_keys", || {
        for i in 0..512 {
            queue_player_leaderboard_refresh(
                black_box(&mut large),
                black_box(&long_keys[i % long_keys.len()]),
                black_box(i),
            );
        }
    });
    let mut state = PlayerLeaderboardCacheState::default();
    for key in &keys {
        state.in_flight.insert(key.clone(), 5);
        state.pending_refresh.insert(key.clone(), 10);
    }
    let now = Instant::now();
    measure("queue/request_state", || {
        for i in 0..512 {
            black_box(state.request_leaderboard(
                black_box(&keys[i % keys.len()]),
                black_box(10 + i % 32),
                black_box(true),
                black_box(now),
            ));
        }
    });
    let tags = tag_inputs(2);
    measure("compose/row", || {
        for i in 0..128 {
            black_box((
                arrowcloud_pane_kind_from_type(black_box(&panes[i])),
                scorebox_machine_tag(black_box(tags[i].0.as_deref()), black_box(&tags[i].1)),
                format_scorebox_score_value(black_box(9876.5 + (i % 100) as f64)),
            ));
        }
    });
    measure("control/rank", || {
        for i in 0..512 {
            black_box(format_scorebox_rank(black_box(i)));
        }
    });
}
