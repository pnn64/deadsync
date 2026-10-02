//! Parent parity, allocation budgets and CPU/throughput controls.
use super::*;
use std::hint::black_box;

include!("attack_preparation_baseline.rs");

#[test]
fn borrowed_normalized_keys_match_parent_across_spellings_and_buffer_boundaries() {
    let mut tokens: Vec<String> = [
        "",
        "drunk",
        "drunk2",
        "Drunk",
        "DRUNK",
        "2drunk",
        "234",
        "_drunk",
        "drunk-speed",
        "drunk speed",
        "drunké",
        "édrunk",
        "😀Drunk",
        "no reverse",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for size in [127, 128, 129, 512, 4096] {
        tokens.push("a".repeat(size));
        tokens.push(format!("A{}", "b".repeat(size)));
        tokens.push(format!("12_{}", "ab-".repeat(size)));
    }
    // Valid UTF-8 combinations exercise leading digits, punctuation and Unicode
    // filtering without assuming the normalized form of a token.
    for seed in 0..256 {
        let alphabet = ["a", "Z", "0", "8", "-", "_", " ", "%", "é", "😀"];
        let mut token = String::new();
        let mut state = seed as u32 + 1;
        for _ in 0..(seed % 197) {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            token.push_str(alphabet[state as usize % alphabet.len()]);
        }
        tokens.push(token);
    }
    for token in tokens {
        let mut old_buffer = [0; ATTACK_KEY_STACK_BYTES];
        let mut new_buffer = [0; ATTACK_KEY_STACK_BYTES];
        let old = old_buffered_attack_token_key(&token, &mut old_buffer);
        let new = buffered_attack_token_key(&token, &mut new_buffer);
        assert_eq!(old.as_str(), new.as_str(), "{token:?}");
        assert_eq!(new.as_str(), attack_token_key(&token));
    }
    let normalized = "drunkspeed2";
    let mut buffer = [0; ATTACK_KEY_STACK_BYTES];
    let key = buffered_attack_token_key(normalized, &mut buffer);
    assert!(matches!(key, BufferedAttackKey::Borrowed(_)));
    assert_eq!(key.as_str().as_ptr(), normalized.as_ptr());
}

#[test]
fn full_modifier_parsers_match_parent_for_order_aliases_clearall_and_float_edges() {
    for mods in [
        "",
        "drunk",
        "Drunk",
        "50% drunk, 20% reverse, no tiny",
        "drunk 50, reverse 20, tiny no",
        "*2.5 50% drunk, *0.25 10% mini",
        "50 drunk, 25 drunk, clearall, -0% drunk, no reverse",
        "50% Bumpy-2, 10% dark3, 25% stealth4, 2x, c400",
        "NaN% drunk, inf% tiny, -inf% reverse",
        "cNaN, Xinf, M0",
        "drunké 50, *2 25% drunk-速度, 12drunk 25, incoming 50",
    ] {
        let old = old_parse_attack_mods(mods);
        let new = parse_attack_mods(mods);
        assert_eq!(format!("{old:?}"), format!("{new:?}"), "chart {mods}");
        let old = old_parse_song_lua_runtime_mods_core::<true>(mods);
        let new = parse_song_lua_runtime_mods(mods);
        assert_eq!(format!("{old:?}"), format!("{new:?}"), "Lua {mods}");
    }
}

fn raw_attacks(count: usize, mode: &str) -> String {
    (0..count)
        .map(|i| match mode {
            "invalid" => "TIME=NaN:LEN=1:MODS=Drunk;".to_owned(),
            "zero" => format!("TIME={i}:LEN=0:MODS=Drunk;"),
            "unsupported" => format!("TIME={i}:LEN=1:MODS=Unknown;"),
            "mixed" if i % 3 == 0 => format!("TIME={i}:LEN=0:MODS=Drunk;"),
            _ => format!("TIME={i}:LEN=1:MODS=50% Drunk, no Reverse;"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_masks_equal(old: &[AttackMaskWindow], new: &[AttackMaskWindow]) {
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
}

#[test]
fn lazy_chart_mask_output_preserves_modes_parsing_order_and_tiny_capacity() {
    for mode in ["valid", "invalid", "zero", "unsupported", "mixed"] {
        let raw = raw_attacks(129, mode);
        for attack_mode in [
            GameplayAttackMode::Off,
            GameplayAttackMode::On,
            GameplayAttackMode::Random,
        ] {
            for player in [0, 1] {
                let old = old_build_attack_mask_windows_for_mode(
                    Some(&raw),
                    attack_mode,
                    player,
                    17,
                    120.0,
                );
                let new =
                    build_attack_mask_windows_for_mode(Some(&raw), attack_mode, player, 17, 120.0);
                assert_masks_equal(&old, &new);
            }
        }
    }
    for raw in [
        "",
        "junk",
        "TiMe=0:LEN=1:MODS=Drunk",
        "TIME=0:END=2:MODS=drunk,TIME=1:LEN=1:MODS=reverse",
        "TIME=0:LEN=1:MODS=drunk:TIME=1:LEN=1:MODS=clearall",
        "TIME=-0:LEN=-1:MODS=Drunk",
        "TIME=1:LEN=inf:MODS=Drunk,",
        "TIME=0:LEN=1:MODS=drunké",
    ] {
        let old =
            old_build_attack_mask_windows_for_mode(Some(raw), GameplayAttackMode::On, 0, 0, 10.0);
        let new = build_attack_mask_windows_for_mode(Some(raw), GameplayAttackMode::On, 0, 0, 10.0);
        assert_masks_equal(&old, &new);
    }
    let raw = "TIME=0:LEN=1:MODS=Drunk";
    let old = old_build_attack_mask_windows_for_mode(Some(raw), GameplayAttackMode::On, 0, 0, 10.0);
    let new = build_attack_mask_windows_for_mode(Some(raw), GameplayAttackMode::On, 0, 0, 10.0);
    assert_eq!(old.len(), 1);
    assert_eq!(old.capacity(), new.capacity());
}

fn columns(count: usize, mode: &str) -> Vec<SongLuaColumnOffsetWindowRuntime> {
    let mut out: Vec<_> = (0..count)
        .map(|i| {
            build_song_lua_column_offset_window_runtime(
                if mode == "mixed" { i % 8 } else { 0 },
                if mode == "mixed" && i % 3 == 0 {
                    SongLuaColumnTransformTarget::OffsetX
                } else {
                    SongLuaColumnTransformTarget::OffsetY
                },
                if mode == "ties" {
                    (i % 8) as f32 * 0.00025
                } else {
                    i as f32 * 0.03125
                },
                i as f32 * 0.03125 + 0.015625,
                i as f32 * 0.03125 + 0.125,
                i as f32,
                -(i as f32),
                Some("inOutCubic"),
                Some(1.0),
                None,
            )
        })
        .collect();
    if mode == "reverse" {
        out.reverse();
    }
    if mode == "ties" {
        out.sort_by(|a, b| a.start_second.total_cmp(&b.start_second));
    }
    out
}

fn assert_column_bits(
    old: &[SongLuaColumnOffsetWindowRuntime],
    new: &[SongLuaColumnOffsetWindowRuntime],
) {
    for (a, b) in old.iter().zip(new) {
        assert_eq!(
            a.sustain_end_second.to_bits(),
            b.sustain_end_second.to_bits()
        );
        assert_eq!(a.start_second.to_bits(), b.start_second.to_bits());
        assert_eq!(a.end_second.to_bits(), b.end_second.to_bits());
        assert_eq!(a.from_y.to_bits(), b.from_y.to_bits());
        assert_eq!(a.to_y.to_bits(), b.to_y.to_bits());
        assert_eq!(a.target, b.target);
        assert_eq!(a.column, b.column);
    }
}

#[test]
fn direct_column_tails_match_parent_for_groups_ties_epsilon_and_nonfinite_values() {
    for size in [0, 1, 8, 64, 256, 257, 1024, 8192] {
        for mode in ["ordered", "reverse", "mixed", "ties"] {
            let input = columns(size, mode);
            let mut old = input.clone();
            let mut new = input.clone();
            old_song_lua_extend_column_offset_tails(&mut old);
            song_lua_extend_column_offset_tails(&mut new);
            assert_column_bits(&old, &new);
            old_song_lua_extend_column_offset_tails(&mut old);
            song_lua_extend_column_offset_tails(&mut new);
            assert_column_bits(&old, &new);
            for (i, (start, end)) in [
                (-0.0, 0.0),
                (0.0, -0.0),
                (0.000999, 0.001),
                (0.001, 0.0),
                (f32::MIN, f32::MAX),
                (f32::MAX, f32::MAX),
            ]
            .into_iter()
            .enumerate()
            .take(size)
            {
                old[i].start_second = start;
                old[i].end_second = end;
                new[i].start_second = start;
                new[i].end_second = end;
            }
            old_song_lua_extend_column_offset_tails(&mut old);
            song_lua_extend_column_offset_tails(&mut new);
            assert_column_bits(&old, &new);
            if size > 0 {
                for start in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                    old[0].start_second = start;
                    new[0].start_second = start;
                    old_song_lua_extend_column_offset_tails(&mut old);
                    song_lua_extend_column_offset_tails(&mut new);
                    assert_column_bits(&old, &new);
                }
            }
        }
    }
}

#[test]
fn normalized_keys_empty_masks_and_ordered_column_tails_meet_heap_budgets() {
    let long = "a".repeat(4096);
    perf::assert_no_churn(|| {
        let mut buffer = [0; ATTACK_KEY_STACK_BYTES];
        black_box(buffered_attack_token_key(&long, &mut buffer));
    });
    perf::assert_reduced_churn(
        || {
            let mut buffer = [0; ATTACK_KEY_STACK_BYTES];
            black_box(old_buffered_attack_token_key(&long, &mut buffer));
        },
        || {
            let mut buffer = [0; ATTACK_KEY_STACK_BYTES];
            black_box(buffered_attack_token_key(&long, &mut buffer));
        },
    );
    for mode in ["invalid", "zero", "unsupported"] {
        let raw = raw_attacks(1024, mode);
        perf::assert_no_churn(|| {
            assert!(
                build_attack_mask_windows_for_mode(Some(&raw), GameplayAttackMode::On, 0, 0, 10.0)
                    .is_empty()
            )
        });
    }
    let raw = "TIME=0:LEN=1:MODS=Drunk";
    perf::assert_churn_budget(1, std::mem::size_of::<AttackMaskWindow>(), || {
        black_box(build_attack_mask_windows_for_mode(
            Some(raw),
            GameplayAttackMode::On,
            0,
            0,
            10.0,
        ));
    });
    let mut old = columns(4096, "ordered");
    let mut new = old.clone();
    perf::assert_no_churn(|| song_lua_extend_column_offset_tails(&mut new));
    perf::assert_reduced_churn(
        || old_song_lua_extend_column_offset_tails(&mut old),
        || song_lua_extend_column_offset_tails(&mut new),
    );
    new.reverse();
    perf::assert_churn_budget(1, new.len() * std::mem::size_of::<usize>(), || {
        song_lua_extend_column_offset_tails(&mut new)
    });
}

#[test]
#[ignore = "manual release comparison with separate CPU and heap samples"]
fn benchmark_attack_preparation() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, token) in [
        ("short", "drunk".to_owned()),
        ("medium", "confusionoffset20".to_owned()),
        ("long", "a".repeat(512)),
        ("uppercase", "DRUNKSPEED".to_owned()),
        ("punctuation", "123_Drunk-Speed".to_owned()),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("key_{name}_{}", if old { "old" } else { "new" }),
                4096,
                1,
                || {
                    let mut buffer = [0; ATTACK_KEY_STACK_BYTES];
                    let key = if old {
                        old_buffered_attack_token_key(black_box(&token), &mut buffer)
                    } else {
                        buffered_attack_token_key(black_box(&token), &mut buffer)
                    };
                    black_box(key.as_str());
                },
            );
        }
    }
    for (name, mods) in [
        (
            "normalized",
            "50% drunk, 20% reverse, no tiny, *2 25% bumpy2, C400",
        ),
        (
            "fallback",
            "50% Drunk-Speed, 20% REVERSE, no Tiny, *2 25% Bumpy-2, C400",
        ),
        ("lua", "50 drunk, 20 reverse, no tiny, *2 25 bumpy2, C400"),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("parse_{name}_{}", if old { "old" } else { "new" }),
                1024,
                5,
                || {
                    if name == "lua" {
                        if old {
                            old_parse_song_lua_runtime_mods_core::<true>(black_box(mods))
                        } else {
                            parse_song_lua_runtime_mods(black_box(mods))
                        }
                    } else if old {
                        old_parse_attack_mods(black_box(mods))
                    } else {
                        parse_attack_mods(black_box(mods))
                    }
                },
            );
        }
    }
    for (size, mode) in [
        (1, "valid"),
        (1024, "valid"),
        (1024, "invalid"),
        (1024, "zero"),
        (1024, "unsupported"),
        (1024, "mixed"),
    ] {
        let raw = raw_attacks(size, mode);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("masks_{size}_{mode}_{}", if old { "old" } else { "new" }),
                64,
                size,
                || {
                    if old {
                        old_build_attack_mask_windows_for_mode(
                            Some(black_box(&raw)),
                            GameplayAttackMode::On,
                            0,
                            17,
                            120.0,
                        )
                    } else {
                        build_attack_mask_windows_for_mode(
                            Some(black_box(&raw)),
                            GameplayAttackMode::On,
                            0,
                            17,
                            120.0,
                        )
                    }
                },
            );
        }
    }
    for (size, mode) in [
        (8, "ordered"),
        (1024, "ordered"),
        (8192, "ordered"),
        (1024, "ties"),
        (1024, "reverse"),
        (1024, "mixed"),
    ] {
        let input = columns(size, mode);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled_with_setup(
                &format!("columns_{size}_{mode}_{}", if old { "old" } else { "new" }),
                128,
                size,
                || input.clone(),
                |out| {
                    if old {
                        old_song_lua_extend_column_offset_tails(black_box(out));
                    } else {
                        song_lua_extend_column_offset_tails(black_box(out));
                    }
                },
            );
        }
    }
}
