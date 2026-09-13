use super::*;
use std::hint::black_box;

#[path = "string_arrays_baseline.rs"]
mod baseline;

fn build(lua: &Lua, values: &[String], borrowed: &[&str], owned: bool, old: bool) -> Table {
    match (owned, old) {
        (false, true) => baseline::create_string_array(lua, borrowed),
        (false, false) => create_string_array(lua, borrowed),
        (true, true) => baseline::create_owned_string_array(lua, values),
        (true, false) => create_owned_string_array(lua, values),
    }
    .unwrap()
}

fn contents(table: &Table) -> Vec<String> {
    table
        .sequence_values::<String>()
        .collect::<mlua::Result<_>>()
        .unwrap()
}

#[test]
fn lua_buffer_pass_string_arrays_preserve_dense_order_values_and_fresh_tables() {
    let lua = Lua::new();
    for len in [0, 1, 2, 3, 4, 8, 17, 33, 129, 1024] {
        let values: Vec<_> = (0..len)
            .map(|i| match i % 4 {
                0 => String::new(),
                1 => "duplicate".to_owned(),
                2 => format!("{i}\0é中🦀"),
                _ => format!("{i}:{}", "long".repeat(128)),
            })
            .collect();
        let borrowed: Vec<_> = values.iter().map(String::as_str).collect();
        for owned in [false, true] {
            let old = build(&lua, &values, &borrowed, owned, true);
            let new = build(&lua, &values, &borrowed, owned, false);
            assert_eq!(contents(&new), values);
            assert_eq!(contents(&new), contents(&old));
            assert_eq!(new.raw_len(), len);
            assert_eq!(new.pairs::<Value, Value>().count(), len);
            assert!(new.metatable().is_none());
            assert_ne!(new.to_pointer(), old.to_pointer());
        }
    }
}

#[test]
fn lua_buffer_pass_string_arrays_own_output_after_sources_and_previous_results_drop() {
    let lua = Lua::new();
    let mut outputs = Vec::new();
    for owned in [false, true] {
        let values: Vec<_> = (0..128)
            .map(|i| format!("{i}\0{}", "é中".repeat(128)))
            .collect();
        let borrowed: Vec<_> = values.iter().map(String::as_str).collect();
        let old = build(&lua, &values, &borrowed, owned, true);
        let new = build(&lua, &values, &borrowed, owned, false);
        outputs.push((new, contents(&old)));
    }
    lua.gc_collect().unwrap();
    for (output, expected) in outputs {
        assert_eq!(contents(&output), expected);
    }
}

#[test]
fn lua_buffer_pass_string_arrays_preserve_independent_mutation_and_append() {
    let lua = Lua::new();
    for len in [0, 1, 3, 17, 129] {
        let values: Vec<_> = (0..len).map(|i| i.to_string()).collect();
        let borrowed: Vec<_> = values.iter().map(String::as_str).collect();
        for owned in [false, true] {
            let old = build(&lua, &values, &borrowed, owned, true);
            let new = build(&lua, &values, &borrowed, owned, false);
            let untouched = build(&lua, &values, &borrowed, owned, false);
            for table in [&old, &new] {
                table.raw_set(len + 1, "appended").unwrap();
                table.raw_set(1, "changed").unwrap();
            }
            assert_eq!(contents(&new), contents(&old));
            assert_eq!(contents(&untouched), values);
        }
    }
}

#[test]
fn lua_buffer_pass_string_arrays_allocate_capacity_once() {
    let lua = Lua::new();
    let interned = lua.create_string("same").unwrap();
    let values = vec!["same".to_owned(); 128];
    let borrowed = vec!["same"; 128];
    for owned in [false, true] {
        drop(build(&lua, &values, &borrowed, owned, false));
        lua.gc_stop();
        crate::perf::assert_churn_budget(2, 128 * 32 + 256, || {
            drop(black_box(build(&lua, &values, &borrowed, owned, false)))
        });
    }
    black_box(interned);
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_buffer_pass_bench_string_arrays() {
    for owned in [false, true] {
        for (kind, count) in [
            ("short", 0),
            ("short", 1),
            ("short", 4),
            ("short", 16),
            ("short", 128),
            ("short", 1024),
            ("long", 128),
            ("long", 1024),
        ] {
            let lua = Lua::new();
            let values: Vec<_> = (0..count)
                .map(|i| {
                    if kind == "long" {
                        format!("{i}\0{}", "é中".repeat(32))
                    } else {
                        format!("item_{i}")
                    }
                })
                .collect();
            let borrowed: Vec<_> = values.iter().map(String::as_str).collect();
            assert_eq!(
                contents(&build(&lua, &values, &borrowed, owned, true)),
                contents(&build(&lua, &values, &borrowed, owned, false))
            );
            lua.gc_collect().unwrap();
            lua.gc_stop();
            let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
                [false, true]
            } else {
                [true, false]
            };
            for old in order {
                crate::perf::measure_sampled(
                    &format!(
                        "string_arrays_{}_{kind}_{count}/{}",
                        if owned { "owned" } else { "borrowed" },
                        if old { "old" } else { "new" }
                    ),
                    16,
                    count.max(1),
                    || {
                        drop(black_box(build(
                            black_box(&lua),
                            black_box(&values),
                            black_box(&borrowed),
                            black_box(owned),
                            black_box(old),
                        )))
                    },
                );
            }
        }
    }
}
