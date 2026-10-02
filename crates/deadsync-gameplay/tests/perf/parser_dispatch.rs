//! Frozen-parent behavior and heap checks for chart and modifier dispatch.
use super::*;
use std::hint::black_box;

include!("parser_dispatch_baseline.rs");

#[test]
fn delimiter_time_search_matches_parent_for_offsets_case_false_prefixes_and_bytes() {
    for position in 0..5 {
        for byte in 0..=u8::MAX {
            let mut raw = *b"TIME=";
            raw[position] = byte;
            assert_eq!(old_find_attack_time(&raw, 0), find_attack_time(&raw, 0));
        }
    }
    for pattern in [b"TIME_", b"TTTTT", b"XIME="] {
        let mut raw = pattern.repeat(40);
        for has_hit in [false, true] {
            if has_hit {
                raw.extend_from_slice(b"tImE=");
            }
            for start in 0..=raw.len() + 1 {
                assert_eq!(
                    old_find_attack_time(&raw, start),
                    find_attack_time(&raw, start),
                    "dense {pattern:?}, hit {has_hit}, offset {start}"
                );
            }
        }
    }
    for raw in [
        "",
        "TIME=",
        "time=0",
        "tImE=0:LEN=1:MODS=drunk",
        "TIME",
        "=TIME",
        "=TI=TIME==",
        "TTTTTIME=",
        "TIME_TIME_TIME=",
        "testTime_TIME=",
        "XTIME=",
        "TIME=0:MODS=TiMe=1",
        "é🙂TIME=0🙂time=2",
        "TIME=NaN:LEN=1:MODS=drunk:TIME=2:END=3:MODS=reverse",
    ] {
        for start in 0..=raw.len() + 1 {
            assert_eq!(
                old_find_attack_time(raw.as_bytes(), start),
                find_attack_time(raw.as_bytes(), start),
                "{raw:?} at {start}"
            );
        }
    }
    for len in [
        0, 1, 4, 5, 15, 16, 31, 32, 63, 64, 65, 128, 512, 4096, 65536,
    ] {
        let mut raw = vec![b'='; len];
        let pivot = raw.len();
        raw.extend_from_slice(b"TiMe=");
        for start in [
            0,
            1,
            pivot.saturating_sub(4),
            pivot,
            pivot + 1,
            raw.len(),
            raw.len() + 1,
            usize::MAX,
        ] {
            assert_eq!(
                old_find_attack_time(&raw, start),
                find_attack_time(&raw, start)
            );
        }
    }
    for seed in 0..256u32 {
        let mut state = seed + 1;
        let mut raw = Vec::new();
        for _ in 0..seed {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            raw.push((state >> 24) as u8);
        }
        raw.extend_from_slice(b"TIME=\xff\0time=");
        for start in 0..=raw.len() + 1 {
            assert_eq!(
                old_find_attack_time(&raw, start),
                find_attack_time(&raw, start),
                "seed {seed}, offset {start}"
            );
        }
    }
    for raw in [
        "TIME=0:LEN=1:MODS=Drunk,TIME=1:END=3:MODS=Reverse",
        "time=0:MODS=drunk:LEN=1:TiMe=NaN:LEN=1:MODS=reverse",
        "é🙂TIME=0:LEN=-1:MODS=drunk🙂,TIME=1:LEN=inf:MODS=reverse",
    ] {
        let old: Vec<_> =
            std::iter::successors(old_find_attack_time(raw.as_bytes(), 0), |&offset| {
                old_find_attack_time(raw.as_bytes(), offset + 5)
            })
            .collect();
        let new: Vec<_> = std::iter::successors(find_attack_time(raw.as_bytes(), 0), |&offset| {
            find_attack_time(raw.as_bytes(), offset + 5)
        })
        .collect();
        assert_eq!(old, new);
        let chunks: Vec<_> = ChartAttackChunks::new(raw).collect();
        let expected: Vec<_> = old
            .iter()
            .enumerate()
            .map(|(i, &offset)| &raw[offset..old.get(i + 1).copied().unwrap_or(raw.len())])
            .collect();
        assert_eq!(chunks, expected);
        for chunk in chunks {
            black_box(parse_chart_attack_chunk(chunk));
        }
    }
}

fn scroll_bits(value: Option<ScrollSpeedSetting>) -> Option<(u8, u32)> {
    value.map(|value| match value {
        ScrollSpeedSetting::XMod(value) => (0, value.to_bits()),
        ScrollSpeedSetting::CMod(value) => (1, value.to_bits()),
        ScrollSpeedSetting::MMod(value) => (2, value.to_bits()),
    })
}

#[test]
fn scroll_dispatch_and_full_parsers_preserve_spelling_order_and_float_bits() {
    for token in [
        "",
        "drunk",
        "50% drunk",
        "no tiny",
        "clearall",
        "mini",
        "confusion",
        "édrunk",
        "🙂drunk",
        "1.5x",
        "1.5X",
        " C400 ",
        "x 1.25",
        "M600",
        "m+600",
        "c.5",
        "Cinvalid",
        "x",
        "c",
        "m",
        "0x",
        "-1x",
        "-0x",
        "+0x",
        "C-0",
        "M-1",
        "CNaN",
        "XNaN",
        "mNaN",
        "cNAN",
        "cNaNé",
        "infx",
        "-infx",
        "Cinf",
        "x+inf",
        "minf",
        "m-inf",
        "X1e39",
        "X1e-45",
        "x1_000",
        "c🙂",
        "ｍ400",
        "\tC 400\n",
        "x\u{2003}2",
        "2éx",
        "1.2xx",
    ] {
        assert_eq!(
            scroll_bits(old_parse_attack_scroll_override(token)),
            scroll_bits(parse_attack_scroll_override(token)),
            "{token}"
        );
    }
    for mods in [
        "",
        "drunk, reverse, tiny, bumpy, boost, flip, hidden, sudden",
        "50% drunk, 20% reverse, no tiny, *2 25% bumpy2, C400",
        "drunk, C400, reverse, 2X, clearall, M600, -0% drunk",
        "50 drunk, 20 reverse, no tiny, *2 25 bumpy2, C400",
        "50% Drunk-Speed, 20% REVERSE, no Tiny, *2 25% Bumpy-2, C400",
        "cNaN, Xinf, M0, 50% drunk, clearall, -inf% tiny",
        "*2 cNaN, *2 xNaN, *2 mNaN, NaN% reverse, 25% édrunk",
    ] {
        assert_eq!(
            format!("{:?}", old_parse_attack_mods(mods)),
            format!("{:?}", parse_attack_mods(mods)),
            "chart {mods}"
        );
        assert_eq!(
            format!("{:?}", old_parse_song_lua_runtime_mods_core::<true>(mods)),
            format!("{:?}", parse_song_lua_runtime_mods(mods)),
            "Lua {mods}"
        );
    }
}

#[test]
fn time_search_and_modifier_dispatch_keep_zero_heap_traffic() {
    let raw = format!("{}TIME=", "a".repeat(65536));
    perf::assert_no_churn(|| {
        black_box(find_attack_time(raw.as_bytes(), 0));
        black_box(parse_attack_scroll_override("50% drunk"));
        black_box(parse_attack_mods(
            "drunk, reverse, tiny, bumpy, boost, flip, hidden, sudden",
        ));
        black_box(parse_song_lua_runtime_mods(
            "drunk, reverse, tiny, bumpy, boost, flip, hidden, sudden",
        ));
    });
}

#[test]
#[ignore = "manual release comparison with separate CPU and heap samples"]
fn benchmark_parser_dispatch() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let hit = |size: usize| {
        let mut raw = vec![b'a'; size - 5];
        raw.extend_from_slice(b"TiMe=");
        raw
    };
    let mut false_prefixes = b"XIME=".repeat(13108);
    false_prefixes.truncate(65536);
    let front = b"TiMe=aaaaaaaaaaa".to_vec();
    let mut false_initials = b"TIME_".repeat(13108);
    false_initials.truncate(65536);
    for (name, raw) in [
        ("16_hit", hit(16)),
        ("32_miss", vec![b'a'; 32]),
        ("4096_hit", hit(4096)),
        ("65536_miss", vec![b'a'; 65536]),
        ("65536_false", false_prefixes),
        ("65536_false_initials", false_initials),
        ("16_front", front),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("scan_{name}_{}", if old { "old" } else { "new" }),
                if raw.len() <= 4096 { 4096 } else { 128 },
                1,
                || {
                    if old {
                        old_find_attack_time(black_box(&raw), 0)
                    } else {
                        find_attack_time(black_box(&raw), 0)
                    }
                },
            );
        }
    }
    for (name, token) in [
        ("ordinary", "50% drunk"),
        ("prefixed", "C400"),
        ("suffix", "1.5x"),
        ("invalid", "Cinvalid"),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("scroll_{name}_{}", if old { "old" } else { "new" }),
                4096,
                1,
                || {
                    if old {
                        old_parse_attack_scroll_override(black_box(token))
                    } else {
                        parse_attack_scroll_override(black_box(token))
                    }
                },
            );
        }
    }
    let mods = "drunk, reverse, tiny, bumpy, boost, flip, hidden, sudden";
    for name in ["chart", "lua"] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("mods_{name}_{}", if old { "old" } else { "new" }),
                1024,
                8,
                || {
                    if name == "chart" {
                        if old {
                            old_parse_attack_mods(black_box(mods))
                        } else {
                            parse_attack_mods(black_box(mods))
                        }
                    } else if old {
                        old_parse_song_lua_runtime_mods_core::<true>(black_box(mods))
                    } else {
                        parse_song_lua_runtime_mods(black_box(mods))
                    }
                },
            );
        }
    }
}
