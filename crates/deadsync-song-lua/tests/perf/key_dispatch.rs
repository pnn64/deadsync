//! Parent parity and allocation/CPU controls for Lua selector keys.
use super::*;
use std::hint::black_box;

include!("key_dispatch_baseline.rs");

#[test]
fn borrowed_overlay_keys_preserve_ascii_only_folding_and_buffer_boundaries() {
    let mut names: Vec<String> = [
        "",
        "music",
        "Music",
        "DIFFUSEBLINK",
        "é🙂music",
        "É🙂music",
        "é🙂Music",
        "中beat",
        " spaces ",
        "\0beat",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for size in [31, 32, 33, 128, 4096] {
        names.push("a".repeat(size));
        names.push(format!("A{}", "a".repeat(size)));
        names.push(format!("{}Z", "a".repeat(size)));
        names.push("é".repeat(size));
    }
    for seed in 0..256u32 {
        let alphabet = ["a", "Z", "é", "É", "🙂", "-", "0", " "];
        let mut state = seed + 1;
        let mut name = String::new();
        for _ in 0..seed % 139 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            name.push_str(alphabet[state as usize % alphabet.len()]);
        }
        names.push(name);
    }
    for name in names {
        let old = old_with_ascii_lowercase_overlay_key(&name, str::to_owned);
        let new = with_ascii_lowercase_overlay_key(&name, str::to_owned);
        assert_eq!(old, new, "{name:?}");
        assert_eq!(new, name.to_ascii_lowercase());
        if !name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            assert!(with_ascii_lowercase_overlay_key(&name, |key| key.as_ptr() == name.as_ptr()));
        }
    }
}

#[test]
fn full_overlay_selectors_and_theme_callbacks_match_parent() {
    let lua = mlua::Lua::new();
    let mut names: Vec<String> = [
        "",
        "music",
        "Music",
        "'beat'",
        "\"beatnooffset\"",
        " timer ",
        "diffuseblink",
        "DiffuseBlink",
        "diffuseramp",
        "horizalign_left",
        "HorizAlign_Right",
        " center ",
        "textglowmode_inner",
        "TextGlowMode_Both",
        "é🙂beat",
        "é🙂BEAT",
        "ScreenVisualOptions",
        "screenvisualoptions",
        "UseImageCache",
        "useimagecache",
        "CasualMaxMeter",
        "ThemeFont",
        "SongSelectBG",
        "UnknownPreference",
        "中",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    names.push(format!("{}beat", "a".repeat(128)));
    names.push(format!("{}BEAT", "a".repeat(128)));
    names.push("é".repeat(128));
    for name in names {
        assert_eq!(
            old_parse_overlay_effect_mode(&name),
            parse_overlay_effect_mode(&name),
            "mode {name}"
        );
        assert_eq!(
            old_parse_overlay_effect_clock(&name),
            parse_overlay_effect_clock(&name),
            "clock {name}"
        );
        assert_eq!(
            old_parse_overlay_text_align(&name),
            parse_overlay_text_align(&name),
            "align {name}"
        );
        assert_eq!(
            old_parse_overlay_text_glow_mode(&name),
            parse_overlay_text_glow_mode(&name),
            "glow {name}"
        );
        assert_eq!(
            old_theme_screen_fallback(&name),
            theme_screen_fallback(&name),
            "screen {name}"
        );
        assert_eq!(
            old_theme_pref_default(&lua, &name).unwrap(),
            theme_pref_default(&lua, &name).unwrap(),
            "pref {name}"
        );
    }
}

#[test]
fn lowercase_selector_keys_and_long_clocks_have_no_heap_churn() {
    let key = "a".repeat(4096);
    perf::assert_no_churn(|| {
        with_ascii_lowercase_overlay_key(&key, |folded| {
            black_box(folded);
        });
        black_box(parse_overlay_effect_clock(&key));
        black_box(parse_overlay_effect_mode("diffuseblink"));
        black_box(parse_overlay_text_align("horizalign_left"));
        black_box(parse_overlay_text_glow_mode("textglowmode_both"));
    });
    perf::assert_reduced_churn(
        || {
            black_box(old_parse_overlay_effect_clock(&key));
        },
        || {
            black_box(parse_overlay_effect_clock(&key));
        },
    );
    let mixed = format!("A{}", "a".repeat(128));
    perf::assert_churn_budget(1, mixed.len(), || {
        with_ascii_lowercase_overlay_key(&mixed, |folded| {
            black_box(folded);
        });
    });
}

#[test]
#[ignore = "manual release comparison with separate CPU and heap samples"]
fn benchmark_key_dispatch() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, key) in [
        ("short_lower", "diffuseblink".to_owned()),
        ("short_mixed", "DiffuseBlink".to_owned()),
        ("unicode", "é🙂music".to_owned()),
        ("long_lower", "a".repeat(128)),
        ("long_mixed", format!("A{}", "a".repeat(127))),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("overlay_key_{name}_{}", if old { "old" } else { "new" }),
                4096,
                1,
                || {
                    let use_key = |folded: &str| {
                        black_box(folded);
                    };
                    if old {
                        old_with_ascii_lowercase_overlay_key(black_box(&key), use_key)
                    } else {
                        with_ascii_lowercase_overlay_key(black_box(&key), use_key)
                    }
                },
            );
        }
    }
    for (name, keys) in [
        (
            "lower",
            [
                "diffuseblink",
                "music",
                "horizalign_left",
                "textglowmode_both",
            ],
        ),
        (
            "mixed",
            [
                "DiffuseBlink",
                "Music",
                "HorizAlign_Left",
                "TextGlowMode_Both",
            ],
        ),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!(
                    "overlay_selectors_{name}_{}",
                    if old { "old" } else { "new" }
                ),
                2048,
                4,
                || {
                    if old {
                        (
                            old_parse_overlay_effect_mode(black_box(keys[0])),
                            old_parse_overlay_effect_clock(black_box(keys[1])),
                            old_parse_overlay_text_align(black_box(keys[2])),
                            old_parse_overlay_text_glow_mode(black_box(keys[3])),
                        )
                    } else {
                        (
                            parse_overlay_effect_mode(black_box(keys[0])),
                            parse_overlay_effect_clock(black_box(keys[1])),
                            parse_overlay_text_align(black_box(keys[2])),
                            parse_overlay_text_glow_mode(black_box(keys[3])),
                        )
                    }
                },
            );
        }
    }
    let clock = format!("{}beat", "a".repeat(124));
    for old in if reverse {
        [false, true]
    } else {
        [true, false]
    } {
        perf::measure_sampled(
            &format!("overlay_clock_long_{}", if old { "old" } else { "new" }),
            4096,
            1,
            || {
                if old {
                    old_parse_overlay_effect_clock(black_box(&clock))
                } else {
                    parse_overlay_effect_clock(black_box(&clock))
                }
            },
        );
    }
}
