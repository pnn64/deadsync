use super::*;
use std::hint::black_box;

#[path = "mod_tokens_baseline.rs"]
mod baseline;

fn normalized(text: &str) -> String {
    crate::player_options::with_normalized_player_option_key(text, str::to_owned)
}

#[test]
fn lua_stream_mod_keys_preserve_ascii_filtering_and_long_unicode_inputs() {
    for text in ["", "No Mines", "MoveX1", "C-Mod!", "éあ💃", "a\0B\t1"] {
        assert_eq!(
            normalized(text),
            baseline::normalize_player_option_key(text)
        );
        assert_eq!(normalize_player_option_key(text), normalized(text));
    }
    let mut seed = 11u64;
    for len in [0, 1, 63, 64, 65, 128, 4096] {
        for _ in 0..100 {
            let text: String = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    char::from_u32((seed >> 32) as u32 % 0x110000).unwrap_or('A')
                })
                .collect();
            assert_eq!(
                normalized(&text),
                baseline::normalize_player_option_key(&text)
            );
        }
        let text = "A-9é".repeat(len);
        assert_eq!(
            normalized(&text),
            baseline::normalize_player_option_key(&text)
        );
    }
    for byte in 0..=127u8 {
        let text = format!("a{}Z", char::from(byte));
        assert_eq!(
            normalized(&text),
            baseline::normalize_player_option_key(&text)
        );
    }
}

fn snapshot(owner: &Table) -> String {
    let mut out = Vec::new();
    for field in [
        "__songlua_player_option_state",
        "__songlua_player_option_speeds",
    ] {
        if let Some(table) = owner.raw_get::<Option<Table>>(field).unwrap() {
            for pair in table.pairs::<String, Value>() {
                let (key, value) = pair.unwrap();
                let value = match value {
                    Value::Number(n) => format!("number:{:x}", n.to_bits()),
                    value => format!("{value:?}"),
                };
                out.push(format!("{field}:{key}:{value}"));
            }
        }
    }
    for key in ["active", "xmod", "cmod", "mmod", "amod", "camod"] {
        out.push(format!(
            "{key}:{:?}",
            owner
                .raw_get::<Value>(format!("__songlua_speedmod_{key}"))
                .unwrap()
        ));
    }
    out.sort();
    out.join("\n")
}

#[test]
fn lua_stream_mod_tokens_preserve_state_speeds_precedence_and_partial_errors() {
    let lua = Lua::new();
    for fail in [false, true] {
        let owners = [lua.create_table().unwrap(), lua.create_table().unwrap()];
        for owner in &owners {
            let state = player_option_state(&lua, owner).unwrap();
            if fail {
                let mt = lua.create_table().unwrap();
                mt.set("__newindex", lua.load("return function(t,k,v) if k == 'tiny' then error('token failed') end rawset(t,k,v) end").eval::<Function>().unwrap()).unwrap();
                state.set_metatable(Some(mt)).unwrap();
            }
        }
        for text in [
            "*2 50% Drunk, no mines, 0% NoMines, -0% reverse",
            "C400, 1.5x, CA250, 70% TINY, *-3 25% No Holds",
            "inf mini, -inf moveX1, NaN MoveY2, *NaN 99% Mini",
            " , %$éあ, éR-éV-éR-S-é, +3E2 X",
            &format!("50% {}", "Aé-9".repeat(90)),
        ] {
            let old = baseline::apply_player_options_string(&lua, &owners[0], text);
            let new = apply_player_options_string(&lua, &owners[1], text);
            assert_eq!(
                old.as_ref().err().map(ToString::to_string),
                new.as_ref().err().map(ToString::to_string)
            );
            assert_eq!(snapshot(&owners[0]), snapshot(&owners[1]), "{text}");
        }
    }
}

#[test]
fn lua_stream_mod_normalization_and_warmed_tokens_have_no_churn() {
    let lua = Lua::new();
    let owner = lua.create_table().unwrap();
    let text = "50% Drunk, No Mines, *2 25% MoveX1, 0% No Holds";
    apply_player_options_string(&lua, &owner, text).unwrap();
    lua.gc_stop();
    let long = "a9".repeat(1024);
    crate::perf::assert_no_churn(|| {
        for text in ["reverse", "No Mines", "éA-9", long.as_str()] {
            crate::player_options::with_normalized_player_option_key(black_box(text), |key| {
                black_box(key);
            });
        }
        apply_player_options_string(&lua, &owner, black_box(text)).unwrap();
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_stream_bench_mod_tokens() {
    let long = "Aé-9".repeat(90);
    for (name, inputs) in [
        (
            "mod_keys_lower",
            vec!["reverse", "drunk", "movex1", "nomines"],
        ),
        (
            "mod_keys_mixed",
            vec!["Reverse", "DrunkSpeed", "MoveX1", "No Mines"],
        ),
        (
            "mod_keys_unicode",
            vec!["éR-éV-éR-S-é", "éあ💃", "\0A_1", ""],
        ),
        ("mod_keys_long", vec![long.as_str()]),
    ] {
        for input in &inputs {
            assert_eq!(
                normalized(input),
                baseline::normalize_player_option_key(input)
            );
        }
        for old in order() {
            crate::perf::measure_sampled(
                &format!("{name}/{}", if old { "old" } else { "new" }),
                512,
                inputs.len() * 16,
                || {
                    for _ in 0..16 {
                        for input in &inputs {
                            if old {
                                black_box(baseline::normalize_player_option_key(black_box(input)));
                            } else {
                                crate::player_options::with_normalized_player_option_key(
                                    black_box(input),
                                    |key| {
                                        black_box(key);
                                    },
                                );
                            }
                        }
                    }
                },
            );
        }
    }
    for (name, text) in [
        (
            "mod_apply_lower",
            "50% drunk, reverse, *2 25% movex1, 0% nomines",
        ),
        (
            "mod_apply_mixed",
            "50% Drunk, Reverse, *2 25% MoveX1, 0% No Mines",
        ),
        ("mod_apply_empty", " , , , "),
        ("mod_apply_long", long.as_str()),
    ] {
        let lua = Lua::new();
        let owner = lua.create_table().unwrap();
        baseline::apply_player_options_string(&lua, &owner, text).unwrap();
        let expected = snapshot(&owner);
        apply_player_options_string(&lua, &owner, text).unwrap();
        assert_eq!(snapshot(&owner), expected);
        lua.gc_stop();
        for old in order() {
            crate::perf::measure_sampled(
                &format!("{name}/{}", if old { "old" } else { "new" }),
                128,
                text.split(',').count() * 16,
                || {
                    for _ in 0..16 {
                        if old {
                            baseline::apply_player_options_string(&lua, &owner, black_box(text))
                                .unwrap();
                        } else {
                            apply_player_options_string(&lua, &owner, black_box(text)).unwrap();
                        }
                    }
                    black_box(&owner);
                },
            );
        }
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}
