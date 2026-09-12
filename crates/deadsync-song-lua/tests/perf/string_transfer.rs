use super::*;
use std::hint::black_box;

#[path = "string_transfer_baseline.rs"]
mod baseline;

fn stringify(lua: &Lua, args: &MultiValue, old: bool) -> mlua::Result<Value> {
    if old {
        baseline::stringify_lua_table(lua, args)
    } else {
        stringify_lua_table(lua, args)
    }
}

fn strings(value: Value) -> Vec<String> {
    let Value::Table(table) = value else {
        panic!("expected table")
    };
    table
        .sequence_values::<String>()
        .collect::<mlua::Result<_>>()
        .unwrap()
}

fn args(lua: &Lua, table: &Table, form: Option<&str>) -> MultiValue {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    if let Some(form) = form {
        args.push_back(Value::String(lua.create_string(form).unwrap()));
    }
    args
}

#[test]
fn lua_memory_string_transfer_preserves_long_unicode_strings_and_lifetimes() {
    let lua = Lua::new();
    let expected: Vec<_> = [0, 1, 40, 41, 256, 4096]
        .into_iter()
        .map(|n| format!("a\0{}", "é中🦀".repeat(n)))
        .collect();
    let input = lua
        .create_sequence_from(expected.iter().map(String::as_str))
        .unwrap();
    let arguments = args(&lua, &input, None);
    let old = stringify(&lua, &arguments, true).unwrap();
    let new = stringify(&lua, &arguments, false).unwrap();
    assert_eq!(strings(old), expected);
    drop(arguments);
    drop(input);
    lua.gc_collect().unwrap();
    assert_eq!(strings(new), expected);
}

#[test]
fn lua_memory_string_transfer_preserves_callback_strings_errors_and_order() {
    for invalid in [false, true] {
        let mut results = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            lua.globals().set("invalid", invalid).unwrap();
            let input = lua
                .load(
                    r#"
                values = {1,2,3}; calls = {}
                local text = string.rep('é中', 128) .. '\0'
                string.format = function(form, value)
                    calls[#calls+1] = value
                    if value == 1 then values[2] = 20 end
                    if value == 20 and invalid then return string.char(255) end
                    return text .. value
                end
                return values
            "#,
                )
                .set_name("string_transfer")
                .eval::<Table>()
                .unwrap();
            let result = stringify(&lua, &args(&lua, &input, Some("ignored")), old);
            assert_eq!(result.is_err(), invalid);
            let result = result.map(strings).map_err(|e| e.to_string());
            let calls = lua
                .globals()
                .get::<Table>("calls")
                .unwrap()
                .sequence_values::<i64>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(calls, if invalid { vec![1, 20] } else { vec![1, 20, 3] });
            results.push((result, calls));
        }
        assert_eq!(results[0], results[1]);
    }
}

#[test]
fn lua_memory_string_transfer_preserves_non_string_coercions_and_invalid_input() {
    let lua = Lua::new();
    let input = lua
        .create_sequence_from([
            Value::Boolean(true),
            Value::Integer(i64::MAX),
            Value::Number(-0.0),
            Value::Number(f64::NAN),
            Value::Table(lua.create_table().unwrap()),
        ])
        .unwrap();
    for form in [None, Some("%g")] {
        let arguments = args(&lua, &input, form);
        assert_eq!(
            strings(stringify(&lua, &arguments, true).unwrap()),
            strings(stringify(&lua, &arguments, false).unwrap())
        );
    }
    input
        .raw_set(2, lua.create_string([255, 0, 254]).unwrap())
        .unwrap();
    let arguments = args(&lua, &input, None);
    assert_eq!(
        stringify(&lua, &arguments, true).unwrap_err().to_string(),
        stringify(&lua, &arguments, false).unwrap_err().to_string()
    );
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_memory_bench_string_transfer() {
    for (name, count, form, text) in [
        ("empty", 0, Some("%.2f"), false),
        ("one", 1, Some("%.2f"), false),
        ("numeric_32", 32, Some("%.2f"), false),
        ("numeric_256", 256, Some("%.2f"), false),
        ("numeric_1024", 1024, Some("%.2f"), false),
        ("unformatted_256", 256, None, false),
        ("strings_256", 256, Some("%.2f"), true),
        ("long_strings_128", 128, None, true),
        ("long_strings_1024", 1024, None, true),
        ("wide_numeric_256", 256, Some("%96.2f"), false),
    ] {
        let lua = Lua::new();
        let table = lua
            .create_sequence_from((0..count).map(|i| {
                if text {
                    Value::String(
                        lua.create_string(if name.starts_with("long") {
                            format!("text_{i}_{}", "é中🦀".repeat(32))
                        } else {
                            format!("text_{i}")
                        })
                        .unwrap(),
                    )
                } else {
                    Value::Number(i as f64 * 0.25)
                }
            }))
            .unwrap();
        let args = args(&lua, &table, form);
        let warm = stringify(&lua, &args, true).unwrap();
        assert_eq!(
            strings(warm.clone()),
            strings(stringify(&lua, &args, false).unwrap())
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
                &format!("string_transfer_{name}/{}", if old { "old" } else { "new" }),
                64,
                count.max(1),
                || {
                    drop(black_box(
                        stringify(black_box(&lua), black_box(&args), black_box(old)).unwrap(),
                    ));
                },
            );
        }
        black_box(warm);
    }
}
