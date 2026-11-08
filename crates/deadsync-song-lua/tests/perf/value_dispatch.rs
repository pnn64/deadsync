use super::*;
use mlua::{Lua, Table};

#[allow(dead_code)]
#[path = "value_dispatch_before.rs"]
mod before;

fn case_variants(text: &str) -> Vec<String> {
    let mut variants = vec![String::new()];
    for ch in text.chars() {
        let mut next = Vec::new();
        for prefix in variants {
            let mut lower = prefix.clone();
            lower.push(ch.to_ascii_lowercase());
            next.push(lower);
            if ch.is_ascii_alphabetic() {
                let mut upper = prefix;
                upper.push(ch.to_ascii_uppercase());
                next.push(upper);
            }
        }
        variants = next;
    }
    variants
}

#[test]
fn ascii_enum_readers_keep_case_variants_and_strict_boundaries() {
    let lua = Lua::new();
    for (text, expected) in [("len", SongLuaSpanMode::Len), ("end", SongLuaSpanMode::End)] {
        for text in case_variants(text) {
            let value = Value::String(lua.create_string(text).unwrap());
            assert_eq!(read_span_mode(value.clone()), Some(expected));
            assert_eq!(read_span_mode(value.clone()), before::read_span_mode(value));
        }
    }
    for (text, expected) in [
        ("P1", 0),
        ("P2", 1),
        ("PlayerNumber_P1", 0),
        ("PlayerNumber_P2", 1),
    ] {
        for text in case_variants(text) {
            let value = Value::String(lua.create_string(&text).unwrap());
            assert_eq!(player_index_from_value(&value), Some(expected), "{text}");
            assert_eq!(
                player_index_from_value(&value),
                before::player_index_from_value(&value)
            );
        }
    }
    for text in [
        "",
        " len",
        "end ",
        "\u{2003}len",
        "P1 ",
        " P2",
        "PlayerNumber_P3",
        "PlayerNumber_P1\0",
        "len\0",
        "é曲",
        "Ｐ１",
    ] {
        let value = Value::String(lua.create_string(text).unwrap());
        assert_eq!(read_span_mode(value.clone()), None, "{text:?}");
        assert_eq!(player_index_from_value(&value), None, "{text:?}");
        assert_eq!(
            read_span_mode(value.clone()),
            before::read_span_mode(value.clone())
        );
        assert_eq!(
            player_index_from_value(&value),
            before::player_index_from_value(&value)
        );
    }
    for bytes in [
        vec![0xff; 4096],
        vec![0xed, 0xa0, 0x80],
        vec![0xc0, 0x80],
        vec![b'P', b'1', 0xff],
        vec![b'l', b'e', 0xff],
        vec![0xe2, 0x82],
    ] {
        let value = Value::String(lua.create_string(bytes).unwrap());
        assert_eq!(read_span_mode(value.clone()), None);
        assert_eq!(player_index_from_value(&value), None);
    }
    // Mutate each accepted spelling at every byte, including non-ASCII bytes.
    for name in [
        "len",
        "end",
        "P1",
        "P2",
        "PlayerNumber_P1",
        "PlayerNumber_P2",
    ] {
        for index in 0..name.len() {
            for byte in 0..=255 {
                let mut bytes = name.as_bytes().to_vec();
                bytes[index] = byte;
                let value = Value::String(lua.create_string(bytes).unwrap());
                assert_eq!(
                    read_span_mode(value.clone()),
                    before::read_span_mode(value.clone())
                );
                assert_eq!(
                    player_index_from_value(&value),
                    before::player_index_from_value(&value)
                );
            }
        }
    }
    for value in [
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(-1),
        Value::Integer(0),
        Value::Integer(1),
        Value::Integer(2),
        Value::Integer(i64::MAX),
        Value::Number(-0.0),
        Value::Number(0.5),
        Value::Number(1.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Table(lua.create_table().unwrap()),
    ] {
        assert_eq!(
            read_span_mode(value.clone()),
            before::read_span_mode(value.clone())
        );
        assert_eq!(
            player_index_from_value(&value),
            before::player_index_from_value(&value)
        );
    }
    assert_eq!(player_index_from_value(&Value::Integer(1)), Some(1));
    assert_eq!(player_index_from_value(&Value::Number(0.5)), None);
}

#[test]
fn outlined_string_equality_keeps_mixed_search_and_table_identity() {
    let lua = Lua::new();
    let other = Lua::new();
    let table = lua.create_table().unwrap();
    table
        .set_metatable(Some(
            lua.load("return { __eq=function() error('must not run') end }")
                .eval()
                .unwrap(),
        ))
        .unwrap();
    let function = lua.create_function(|_, ()| Ok(())).unwrap();
    let values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(0),
        Value::Integer(1),
        Value::Number(1.0),
        Value::Integer(9007199254740993),
        Value::Number(9007199254740992.0),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::String(lua.create_string("").unwrap()),
        Value::String(lua.create_string("é曲\0").unwrap()),
        Value::String(other.create_string("é曲\0").unwrap()),
        Value::String(lua.create_string([0xff; 4096]).unwrap()),
        Value::Table(table.clone()),
        Value::Table(table),
        Value::Table(lua.create_table().unwrap()),
        Value::Function(function.clone()),
        Value::Function(function),
    ];
    lua.gc_collect().unwrap();
    other.gc_collect().unwrap();
    for left in &values {
        for right in &values {
            assert_eq!(
                lua_values_equal(left, right),
                before::lua_values_equal(left, right)
            );
        }
        assert_eq!(
            values
                .iter()
                .position(|right| lua_values_equal(left, right)),
            values
                .iter()
                .position(|right| before::lua_values_equal(left, right))
        );
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for (index, value) in values.iter().cycle().take(60).enumerate() {
        if !old
            .iter()
            .any(|&prior| before::lua_values_equal(&values[prior], value))
        {
            old.push(index % values.len());
        }
        if !new
            .iter()
            .any(|&prior| lua_values_equal(&values[prior], value))
        {
            new.push(index % values.len());
        }
    }
    assert_eq!(old, new);
}

#[test]
fn enum_and_equality_callbacks_keep_return_values_and_errors() {
    fn run(current: bool) -> mlua::Result<Vec<String>> {
        let lua = Lua::new();
        lua.globals().set(
            "span",
            lua.create_function(move |_, value: Value| {
                Ok(
                    match if current {
                        read_span_mode(value)
                    } else {
                        before::read_span_mode(value)
                    } {
                        Some(SongLuaSpanMode::Len) => Some("len"),
                        Some(SongLuaSpanMode::End) => Some("end"),
                        None => None,
                    },
                )
            })?,
        )?;
        lua.globals().set(
            "player",
            lua.create_function(move |_, value: Value| {
                Ok(if current {
                    player_index_from_value(&value)
                } else {
                    before::player_index_from_value(&value)
                })
            })?,
        )?;
        lua.globals().set(
            "find",
            lua.create_function(move |_, (needle, table): (Value, Table)| {
                for pair in table.pairs::<Value, Value>() {
                    let (key, value) = pair?;
                    if if current {
                        lua_values_equal(&needle, &value)
                    } else {
                        before::lua_values_equal(&needle, &value)
                    } {
                        return Ok(key);
                    }
                }
                Ok(Value::Nil)
            })?,
        )?;
        let mut results = Vec::new();
        for source in [
            "return tostring(span('LeN'))..':'..tostring(span('END'))..':'..tostring(span(' len'))",
            "return tostring(player('pLaYeRnUmBeR_p2'))..':'..tostring(player(0))..':'..tostring(player(0.5))",
            "return tostring(span(string.char(255)))..':'..tostring(player(string.char(255)))",
            "return tostring(find(3,{1,2,3,4}))..':'..tostring(find('b',{'a','b','c'}))",
            "local x=string.char(255); return tostring(find(x,{x,x}))",
            "local x={}; return tostring(find(x,{{},x}))",
            "return find(1,false)",
        ] {
            results.push(
                match lua
                    .load(source)
                    .set_name("@value-dispatch.lua")
                    .eval::<String>()
                {
                    Ok(value) => value,
                    Err(error) => error.to_string(),
                },
            );
        }
        Ok(results)
    }
    let old = run(false).unwrap();
    let new = run(true).unwrap();
    assert_eq!(old, new);
    assert_eq!(
        &new[..6],
        ["len:end:nil", "1:0:nil", "nil:nil", "3:2", "nil", "2"]
    );
    assert!(new[6].contains("bad argument"));
}

#[test]
fn ascii_enum_rejections_remove_error_allocation_churn() {
    use crate::draw_capture::perf::allocations;
    let lua = Lua::new();
    for bytes in [
        b"len".to_vec(),
        b"P1".to_vec(),
        b"PlayerNumber_P2".to_vec(),
        "é曲".as_bytes().to_vec(),
        vec![0xff; 4096],
    ] {
        let invalid = std::str::from_utf8(&bytes).is_err();
        let value = Value::String(lua.create_string(bytes).unwrap());
        std::hint::black_box(before::read_span_mode(value.clone()));
        std::hint::black_box(read_span_mode(value.clone()));
        std::hint::black_box(before::player_index_from_value(&value));
        std::hint::black_box(player_index_from_value(&value));
        let (_, old) = allocations::measure(|| {
            std::hint::black_box(before::read_span_mode(value.clone()));
            std::hint::black_box(before::player_index_from_value(&value));
        });
        let (_, new) = allocations::measure(|| {
            std::hint::black_box(read_span_mode(value.clone()));
            std::hint::black_box(player_index_from_value(&value));
            std::hint::black_box(lua_values_equal(&value, &value));
        });
        assert_eq!(new, allocations::Churn::default());
        if invalid {
            assert_eq!(old.allocs, 4);
            assert_eq!(old.reallocs, 2);
        } else {
            assert_eq!(old, allocations::Churn::default());
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_value_dispatch() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    fn measure<W: FnMut()>(
        name: &str,
        current: bool,
        unit: &str,
        input_bytes: usize,
        count: usize,
        mut setup: impl FnMut() -> W,
    ) {
        let mut samples = Vec::new();
        for sample in 0..10 {
            let mut work = setup();
            let start = Instant::now();
            let start_cycles = cycles();
            let (_, churn) = allocations::measure(|| {
                for _ in 0..count {
                    work();
                }
            });
            let used = cycles() - start_cycles;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":count,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":unit,"input_bytes":input_bytes,"baseline_commit":"e400088b0"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, bytes, count) in [
        ("span_len", b"len".to_vec(), 10000),
        ("span_mixed_end", b"EnD".to_vec(), 10000),
        ("span_unknown_ascii", vec![b'x'; 65536], 256),
        ("span_unknown_utf8", "é曲".repeat(4096).into_bytes(), 256),
        ("span_invalid", vec![0xff; 4096], 2000),
        ("span_nil_control", Vec::new(), 100000),
    ] {
        for current in variants {
            measure(name, current, "span_reads", bytes.len(), count, || {
                let lua = Lua::new();
                let value = if name == "span_nil_control" {
                    Value::Nil
                } else {
                    Value::String(lua.create_string(&bytes).unwrap())
                };
                black_box(if current {
                    read_span_mode(value.clone())
                } else {
                    before::read_span_mode(value.clone())
                });
                let fixture = (lua, value);
                move || {
                    let (_, value) = black_box(&fixture);
                    black_box(if current {
                        read_span_mode(value.clone())
                    } else {
                        before::read_span_mode(value.clone())
                    });
                }
            });
        }
    }
    for (name, bytes, count) in [
        ("player_short", b"P1".to_vec(), 10000),
        ("player_long", b"PlayerNumber_P2".to_vec(), 10000),
        ("player_mixed", b"pLaYeRnUmBeR_p1".to_vec(), 10000),
        ("player_unknown_ascii", vec![b'x'; 65536], 256),
        ("player_unknown_utf8", "é曲".repeat(4096).into_bytes(), 256),
        ("player_invalid", vec![0xff; 4096], 2000),
        ("player_integer_control", Vec::new(), 100000),
    ] {
        for current in variants {
            measure(name, current, "player_reads", bytes.len(), count, || {
                let lua = Lua::new();
                let value = if name == "player_integer_control" {
                    Value::Integer(1)
                } else {
                    Value::String(lua.create_string(&bytes).unwrap())
                };
                black_box(if current {
                    player_index_from_value(&value)
                } else {
                    before::player_index_from_value(&value)
                });
                let fixture = (lua, value);
                move || {
                    let (_, value) = black_box(&fixture);
                    black_box(if current {
                        player_index_from_value(value)
                    } else {
                        before::player_index_from_value(value)
                    });
                }
            });
        }
    }
    for (name, bytes, count) in [
        ("dispatch_equal_number", Vec::new(), 100000),
        ("dispatch_equal_boolean", Vec::new(), 100000),
        ("dispatch_equal_short", b"hello".to_vec(), 10000),
        ("dispatch_equal_ascii", vec![b'a'; 65536], 256),
        ("dispatch_equal_utf8", "é曲".repeat(4096).into_bytes(), 256),
        ("dispatch_equal_invalid", vec![0xff; 4096], 2000),
        ("dispatch_equal_table", Vec::new(), 10000),
    ] {
        for current in variants {
            measure(
                name,
                current,
                "value_comparisons",
                bytes.len(),
                count,
                || {
                    let lua = Lua::new();
                    let (left, right) = match name {
                        "dispatch_equal_number" => (Value::Integer(42), Value::Number(42.0)),
                        "dispatch_equal_boolean" => (Value::Boolean(true), Value::Boolean(true)),
                        "dispatch_equal_table" => {
                            let value = Value::Table(lua.create_table().unwrap());
                            (value.clone(), value)
                        }
                        _ => {
                            let value = Value::String(lua.create_string(&bytes).unwrap());
                            (value.clone(), value)
                        }
                    };
                    black_box(if current {
                        lua_values_equal(&left, &right)
                    } else {
                        before::lua_values_equal(&left, &right)
                    });
                    let fixture = (lua, left, right);
                    move || {
                        let (_, left, right) = black_box(&fixture);
                        black_box(if current {
                            lua_values_equal(left, right)
                        } else {
                            before::lua_values_equal(left, right)
                        });
                    }
                },
            );
        }
    }
    for (name, strings, missing, api) in [
        ("search_numbers_late", false, false, false),
        ("search_numbers_missing", false, true, false),
        ("search_strings_late", true, false, false),
        ("search_strings_missing", true, true, false),
        ("search_lua_numbers", false, false, true),
        ("search_lua_strings", true, false, true),
    ] {
        for current in variants {
            measure(
                name,
                current,
                if api {
                    "lua_searches"
                } else {
                    "value_searches"
                },
                if strings { 32 * 9 } else { 0 },
                1000,
                || {
                    let lua = Lua::new();
                    let values: Vec<_> = (0..32)
                        .map(|index| {
                            if strings {
                                Value::String(
                                    lua.create_string(format!("player_{index:02}")).unwrap(),
                                )
                            } else {
                                Value::Integer(index)
                            }
                        })
                        .collect();
                    let needle = if strings {
                        Value::String(
                            lua.create_string(if missing { "player_32" } else { "player_31" })
                                .unwrap(),
                        )
                    } else {
                        Value::Number(if missing { 32.0 } else { 31.0 })
                    };
                    let table = lua.create_sequence_from(values.iter().cloned()).unwrap();
                    // Same pairs/equality path used by the FindInTable binding.
                    let callback = lua
                        .create_function(move |_, (needle, table): (Value, Table)| {
                            for pair in table.pairs::<Value, Value>() {
                                let (key, value) = pair?;
                                if if current {
                                    lua_values_equal(&needle, &value)
                                } else {
                                    before::lua_values_equal(&needle, &value)
                                } {
                                    return Ok(key);
                                }
                            }
                            Ok(Value::Nil)
                        })
                        .unwrap();
                    callback
                        .call::<Value>((needle.clone(), table.clone()))
                        .unwrap();
                    black_box(values.iter().position(|value| {
                        if current {
                            lua_values_equal(&needle, value)
                        } else {
                            before::lua_values_equal(&needle, value)
                        }
                    }));
                    let fixture = (lua, values, needle, table, callback);
                    move || {
                        let (_, values, needle, table, callback) = black_box(&fixture);
                        if api {
                            black_box(
                                callback
                                    .call::<Value>((needle.clone(), table.clone()))
                                    .unwrap(),
                            );
                        } else {
                            black_box(values.iter().position(|value| {
                                if current {
                                    lua_values_equal(needle, value)
                                } else {
                                    before::lua_values_equal(needle, value)
                                }
                            }));
                        }
                    }
                },
            );
        }
    }
}
