use super::*;
use std::hint::black_box;

#[path = "stringify_args_baseline.rs"]
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
fn lua_cleanup_stringify_preserves_mixed_values_and_formats() {
    let lua = Lua::new();
    let table = lua
        .create_sequence_from([
            Value::Integer(17),
            Value::Number(-0.0),
            Value::Boolean(false),
            Value::String(lua.create_string("é\0中").unwrap()),
            Value::Table(lua.create_table().unwrap()),
            Value::Function(lua.create_function(|_, ()| Ok(())).unwrap()),
            Value::Number(f64::INFINITY),
            Value::Number(f64::NAN),
        ])
        .unwrap();
    for form in [None, Some("%.3f"), Some("%q"), Some("x=%g")] {
        let args = args(&lua, &table, form);
        let old = strings(stringify(&lua, &args, true).unwrap());
        let new = strings(stringify(&lua, &args, false).unwrap());
        assert_eq!(old, new);
        assert_eq!(new.len(), 8);
        assert_eq!(&new[2..6], ["false", "é\0中", "", ""]);
    }
}

#[test]
fn lua_cleanup_stringify_preserves_callback_order_arguments_and_mutation() {
    let mut results = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let table = lua
            .load(
                r#"
            values = {1, 2, 3, 4}; calls = {}
            string.format = function(...)
                assert(select('#', ...) == 2)
                local form, value = ...
                calls[#calls+1] = form .. ':' .. value
                if #calls == 1 then
                    values[2] = 20; values[4] = nil; values[5] = 50
                    string.format = function() error('replacement must not run') end
                end
                return form .. '=' .. value
            end
            return values
        "#,
            )
            .set_name("stringify_mutation")
            .eval::<Table>()
            .unwrap();
        let result = strings(stringify(&lua, &args(&lua, &table, Some("tag")), old).unwrap());
        let trace = strings(Value::Table(lua.globals().get("calls").unwrap()));
        assert_eq!(result, ["tag=1", "tag=20", "tag=3"]);
        assert_eq!(trace, ["tag:1", "tag:20", "tag:3"]);
        assert_eq!(table.raw_get::<i64>(5).unwrap(), 50);
        results.push((result, trace));
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn lua_cleanup_stringify_preserves_error_and_lookup_order() {
    for source in [
        "string.format = function() error('format failed') end; return {1,2}",
        "string.format = false; return {}",
        "string = nil; return {}",
        "return {string.char(255)}",
        "string.format = function() return string.char(255) end; return {1}",
    ] {
        let mut errors = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let table = lua
                .load(source)
                .set_name("stringify_error")
                .eval::<Table>()
                .unwrap();
            errors.push(
                stringify(&lua, &args(&lua, &table, Some("%g")), old)
                    .unwrap_err()
                    .to_string(),
            );
        }
        assert_eq!(errors[0], errors[1], "{source}");
    }
}

#[test]
fn lua_cleanup_stringify_preserves_invalid_forms_and_non_table_input() {
    let lua = Lua::new();
    let table = lua.create_sequence_from([12, 34]).unwrap();
    for form in [
        Value::Nil,
        Value::Integer(7),
        Value::Boolean(true),
        Value::String(lua.create_string([255]).unwrap()),
    ] {
        let args = MultiValue::from_vec(vec![Value::Table(table.clone()), form]);
        for old in [true, false] {
            assert_eq!(strings(stringify(&lua, &args, old).unwrap()), ["12", "34"]);
        }
    }
    lua.globals().raw_set("string", Value::Nil).unwrap();
    for args in [
        MultiValue::new(),
        MultiValue::from_vec(vec![Value::Nil]),
        MultiValue::from_vec(vec![Value::Integer(9)]),
    ] {
        for old in [true, false] {
            assert!(stringify(&lua, &args, old).unwrap().is_nil());
        }
    }
}

#[test]
fn lua_cleanup_stringify_preserves_custom_format_return_coercions() {
    let lua = Lua::new();
    let table = lua.create_sequence_from([1, 2, 3]).unwrap();
    for source in [
        "return function() return true end",
        "return function() return 3.25 end",
        "return function() return {} end",
        "return function() end",
    ] {
        lua.globals()
            .get::<Table>("string")
            .unwrap()
            .raw_set("format", lua.load(source).eval::<Function>().unwrap())
            .unwrap();
        let args = args(&lua, &table, Some("ignored"));
        assert_eq!(
            strings(stringify(&lua, &args, true).unwrap()),
            strings(stringify(&lua, &args, false).unwrap())
        );
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_cleanup_bench_stringify() {
    for (name, count, form, text) in [
        ("empty", 0, Some("%.2f"), false),
        ("one", 1, Some("%.2f"), false),
        ("numeric_32", 32, Some("%.2f"), false),
        ("numeric_256", 256, Some("%.2f"), false),
        ("numeric_1024", 1024, Some("%.2f"), false),
        ("unformatted_256", 256, None, false),
        ("strings_256", 256, Some("%.2f"), true),
    ] {
        let lua = Lua::new();
        let table = lua
            .create_sequence_from((0..count).map(|i| {
                if text {
                    Value::String(lua.create_string(format!("text_{i}")).unwrap())
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
                &format!("stringify_{name}/{}", if old { "old" } else { "new" }),
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
