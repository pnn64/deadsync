use super::*;
use std::hint::black_box;

#[path = "split_chars_baseline.rs"]
mod baseline;

fn split(lua: &Lua, text: &str, separator: &str, old: bool) -> Table {
    if old {
        baseline::create_split_table(lua, text, separator)
    } else {
        create_split_table(lua, text, separator)
    }
    .unwrap()
}

fn strings(table: Table) -> Vec<String> {
    table
        .sequence_values::<String>()
        .collect::<mlua::Result<_>>()
        .unwrap()
}

#[test]
fn lua_cleanup_split_preserves_unicode_scalars_and_empty_input() {
    let lua = Lua::new();
    for text in ["", "a", "é中🦀", "e\u{301}", "\0a\0", "\u{10ffff}\u{80}"] {
        let expected = if text.is_empty() {
            vec![String::new()]
        } else {
            text.chars().map(|c| c.to_string()).collect()
        };
        for old in [true, false] {
            let table = split(&lua, text, "", old);
            assert_eq!(table.raw_len(), expected.len());
            assert!(table.metatable().is_none());
            assert_eq!(strings(table), expected);
        }
    }
}

#[test]
fn lua_cleanup_split_preserves_separator_boundaries_and_overlaps() {
    let lua = Lua::new();
    for text in ["", "aaaaa", ",a,,b,", "中é中é中", "a\0b\0", "one line\ntwo"] {
        for separator in [",", "aa", "中é", "\0", "\n", "absent"] {
            let expected: Vec<_> = text.split(separator).collect();
            for old in [true, false] {
                assert_eq!(strings(split(&lua, text, separator, old)), expected);
            }
        }
    }
}

#[test]
fn lua_cleanup_split_matches_parent_for_generated_utf8_and_fresh_tables() {
    let lua = Lua::new();
    let alphabet = ['\0', 'A', 'é', '中', '\u{301}', '🦀'];
    for len in [0, 1, 2, 7, 31, 32, 33, 127, 128, 1024] {
        let text: String = (0..len)
            .map(|i| alphabet[(i * 7 + i / 3) % alphabet.len()])
            .collect();
        let old = split(&lua, &text, "", true);
        let new = split(&lua, &text, "", false);
        assert_ne!(old.to_pointer(), new.to_pointer());
        assert_eq!(strings(old), strings(new.clone()));
        new.raw_set(1, "changed").unwrap();
        assert_eq!(
            strings(split(&lua, &text, "", false)),
            strings(split(&lua, &text, "", true))
        );
    }
}

#[test]
fn lua_cleanup_split_has_only_output_table_allocations_for_interned_characters() {
    let lua = Lua::new();
    let text = "a".repeat(64);
    let warm = split(&lua, &text, "", false);
    lua.gc_stop();
    crate::perf::assert_churn_budget(2, 2048, || {
        drop(black_box(split(&lua, &text, "", false)));
    });
    assert_eq!(warm.raw_len(), 64);
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_cleanup_bench_split() {
    for (name, text, separator) in [
        ("empty", String::new(), ""),
        ("one", "a".to_string(), ""),
        ("ascii_32", "abcdefgh".repeat(4), ""),
        ("ascii_256", "abcdefgh".repeat(32), ""),
        ("unicode_256", "é中🦀\0".repeat(64), ""),
        ("unicode_1024", "é中🦀\0".repeat(256), ""),
        ("separator_256", "a,b,c,d,".repeat(64), ","),
        ("absent_1024", "a".repeat(1024), ","),
    ] {
        assert_eq!(
            strings(split(&Lua::new(), &text, separator, true)),
            strings(split(&Lua::new(), &text, separator, false))
        );
        let lua = Lua::new();
        let warm = split(&lua, &text, separator, true);
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!("split_{name}/{}", if old { "old" } else { "new" }),
                128,
                warm.raw_len().max(1),
                || {
                    drop(black_box(split(
                        black_box(&lua),
                        black_box(&text),
                        black_box(separator),
                        black_box(old),
                    )));
                },
            );
        }
    }
}
