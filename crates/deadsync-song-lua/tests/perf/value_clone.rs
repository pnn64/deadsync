use super::*;
use std::hint::black_box;

#[path = "value_clone_baseline.rs"]
mod baseline;

fn copy(lua: &Lua, value: Value, old: bool) -> mlua::Result<Value> {
    if old {
        baseline::clone_lua_value(lua, value)
    } else {
        clone_lua_value(lua, value)
    }
}

fn fingerprint(value: &Value) -> String {
    match value {
        Value::Table(table) => {
            let mut fields: Vec<_> = table
                .pairs::<Value, Value>()
                .map(|pair| {
                    let (key, value) = pair.unwrap();
                    (fingerprint(&key), fingerprint(&value))
                })
                .collect();
            fields.sort();
            format!(
                "t:[{}]",
                fields
                    .iter()
                    .map(|(key, value)| format!("{}:{key}{}:{value}", key.len(), value.len()))
                    .collect::<String>()
            )
        }
        Value::String(text) => format!("s:{:?}", text.as_bytes().as_ref()),
        Value::Number(number) => format!("n:{:x}", number.to_bits()),
        value => format!("{value:?}"),
    }
}

fn tree(lua: &Lua, width: usize, depth: usize, string_keys: bool) -> Value {
    let table = lua.create_table().unwrap();
    for index in 0..width {
        let key = if string_keys {
            Value::String(lua.create_string(format!("field_{index}")).unwrap())
        } else {
            Value::Integer(index as i64 + 1)
        };
        let value = if depth > 0 {
            tree(lua, width, depth - 1, string_keys)
        } else {
            Value::Number(index as f64 * 0.125)
        };
        table.raw_set(key, value).unwrap();
    }
    Value::Table(table)
}

fn fields(value: &Value) -> usize {
    if let Value::Table(table) = value {
        table
            .pairs::<Value, Value>()
            .map(|pair| {
                let (key, value) = pair.unwrap();
                1 + fields(&key) + fields(&value)
            })
            .sum()
    } else {
        0
    }
}

#[test]
fn lua_traversal_clone_preserves_mixed_keys_values_and_ignores_metatables() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let nested = lua.create_table().unwrap();
    nested.raw_set("nested", 7).unwrap();
    let function = lua.create_function(|_, ()| Ok(42)).unwrap();
    let thread = lua.create_thread(function.clone()).unwrap();
    for (key, value) in [
        (Value::Integer(1), Value::Number(-0.0)),
        (Value::Integer(-7), Value::Number(f64::NAN)),
        (Value::Number(1.5), Value::Number(f64::INFINITY)),
        (Value::Boolean(true), Value::Table(nested.clone())),
        (
            Value::String(lua.create_string([0xff, 0, 0xfe]).unwrap()),
            Value::String(lua.create_string([0, 0xff]).unwrap()),
        ),
        (
            Value::Table(nested.clone()),
            Value::Function(function.clone()),
        ),
        (Value::Function(function), Value::Thread(thread)),
    ] {
        table.raw_set(key, value).unwrap();
    }
    let mt = lua.load("return {__pairs=function() error('pairs invoked') end,__index=function() error('index invoked') end,__newindex=function() error('write invoked') end}").eval::<Table>().unwrap();
    table.set_metatable(Some(mt.clone())).unwrap();
    nested.set_metatable(Some(mt)).unwrap();
    let source = Value::Table(table.clone());
    let expected = fingerprint(&source);
    for old in [true, false] {
        let copied = copy(&lua, source.clone(), old).unwrap();
        assert_eq!(fingerprint(&copied), expected);
        let Value::Table(copied) = copied else {
            panic!("expected table")
        };
        assert_ne!(copied.to_pointer(), table.to_pointer());
        assert!(copied.metatable().is_none());
        let child = copied.raw_get::<Table>(true).unwrap();
        assert_ne!(child.to_pointer(), nested.to_pointer());
        assert!(child.metatable().is_none());
        child.raw_set("nested", 99).unwrap();
        assert_eq!(nested.raw_get::<i32>("nested").unwrap(), 7);
        assert_eq!(fingerprint(&source), expected);
    }
}

#[test]
fn lua_traversal_clone_preserves_shared_input_duplication_and_deep_shapes() {
    let lua = Lua::new();
    let shared = lua.create_table().unwrap();
    shared.raw_set("value", 17).unwrap();
    let parent = lua.create_table().unwrap();
    parent.raw_set(1, shared.clone()).unwrap();
    parent.raw_set(2, shared.clone()).unwrap();
    parent.raw_set(shared, 42).unwrap();
    for old in [true, false] {
        let Value::Table(out) = copy(&lua, Value::Table(parent.clone()), old).unwrap() else {
            panic!("expected table")
        };
        let first = out.raw_get::<Table>(1).unwrap();
        let second = out.raw_get::<Table>(2).unwrap();
        let key = out
            .pairs::<Value, Value>()
            .find_map(|p| match p.unwrap().0 {
                Value::Table(t) => Some(t),
                _ => None,
            })
            .unwrap();
        assert_ne!(first.to_pointer(), second.to_pointer());
        assert_ne!(first.to_pointer(), key.to_pointer());
        first.raw_set("value", 99).unwrap();
        assert_eq!(second.raw_get::<i32>("value").unwrap(), 17);
        assert_eq!(key.raw_get::<i32>("value").unwrap(), 17);
    }
    for (width, depth, strings) in [
        (0, 0, true),
        (128, 0, true),
        (128, 0, false),
        (4, 3, true),
        (1, 96, true),
    ] {
        let source = tree(&lua, width, depth, strings);
        let expected = fingerprint(&source);
        for old in [true, false] {
            assert_eq!(
                fingerprint(&copy(&lua, source.clone(), old).unwrap()),
                expected
            );
        }
    }
}

#[test]
fn lua_traversal_clone_keeps_non_table_identity_without_churn() {
    let lua = Lua::new();
    let function = lua.create_function(|_, ()| Ok(42)).unwrap();
    let values = [
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(i64::MAX),
        Value::Number(f64::NAN),
        Value::String(lua.create_string([0xff]).unwrap()),
        Value::Function(function),
    ];
    for value in values {
        let expected = fingerprint(&value);
        assert_eq!(
            fingerprint(&copy(&lua, value.clone(), true).unwrap()),
            expected
        );
        assert_eq!(fingerprint(&copy(&lua, value, false).unwrap()), expected);
    }
    crate::perf::assert_no_churn(|| {
        for value in [Value::Nil, Value::Integer(17), Value::Number(-0.0)] {
            black_box(clone_lua_value(&lua, value).unwrap());
        }
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_traversal_bench_value_clone() {
    for (width, depth, strings) in [
        (0, 0, false),
        (8, 0, false),
        (64, 0, false),
        (256, 0, false),
        (8, 0, true),
        (64, 0, true),
        (256, 0, true),
        (4, 3, true),
        (4, 3, false),
        (1, 32, true),
    ] {
        let lua = Lua::new();
        let source = tree(&lua, width, depth, strings);
        let expected = fingerprint(&source);
        for old in [true, false] {
            assert_eq!(
                fingerprint(&copy(&lua, source.clone(), old).unwrap()),
                expected
            );
        }
        let units = fields(&source).max(1);
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
                    "clone_{width}_{depth}_{strings}/{}",
                    if old { "old" } else { "new" }
                ),
                32,
                units,
                || {
                    drop(black_box(
                        copy(&lua, source.clone(), black_box(old)).unwrap(),
                    ))
                },
            );
        }
    }
}
