use super::*;
use std::cell::RefCell;
use std::hint::black_box;
use std::rc::Rc;

#[path = "scalar_restore_baseline.rs"]
mod baseline;

fn restore(lua: &Lua, snapshot: Vec<(String, Value)>, old: bool) -> mlua::Result<()> {
    if old {
        baseline::restore_scalar_globals(lua, snapshot)
    } else {
        restore_scalar_globals(lua, snapshot)
    }
}

fn fingerprint(table: &Table) -> Vec<(String, String)> {
    fn value(value: Value) -> String {
        match value {
            Value::String(text) => format!("s:{:?}", text.as_bytes().as_ref()),
            Value::Number(number) => format!("n:{:x}", number.to_bits()),
            value => format!("{value:?}"),
        }
    }
    let mut entries: Vec<_> = table
        .pairs::<Value, Value>()
        .map(|pair| {
            let (k, v) = pair.unwrap();
            (value(k), value(v))
        })
        .collect();
    entries.sort();
    entries
}

#[test]
fn lua_capture_scalar_restore_preserves_values_and_key_rules() {
    let lua = Lua::new();
    let globals = lua.create_table().unwrap();
    lua.set_globals(globals.clone()).unwrap();
    let nested = lua.create_table().unwrap();
    let function = lua.create_function(|_, ()| Ok(42)).unwrap();
    let original: Vec<(String, Value)> = vec![
        ("bool".into(), Value::Boolean(false)),
        ("int".into(), Value::Integer(i64::MAX)),
        ("zero".into(), Value::Number(-0.0)),
        ("nan".into(), Value::Number(f64::NAN)),
        ("inf".into(), Value::Number(f64::INFINITY)),
        (
            "音楽\0key".into(),
            Value::String(lua.create_string([0xff, 0, 0xfe]).unwrap()),
        ),
    ];
    let temporary: Vec<_> = (0..65)
        .map(|i| format!("temporary_{i}_{}", "長".repeat(i)))
        .collect();
    for old in [true, false] {
        globals.clear().unwrap();
        for (key, value) in &original {
            globals.raw_set(key.as_str(), value.clone()).unwrap();
        }
        globals.raw_set("nested", nested.clone()).unwrap();
        globals.raw_set("function", function.clone()).unwrap();
        globals.raw_set(17, "numeric key").unwrap();
        globals.raw_set(true, 12).unwrap();
        let expected = fingerprint(&globals);
        let snapshot = snapshot_scalar_globals(&lua).unwrap();
        for (key, _) in &original {
            globals.raw_set(key.as_str(), nested.clone()).unwrap();
        }
        for key in &temporary {
            globals.raw_set(key.as_str(), 42).unwrap();
        }
        globals.raw_set("new_table", nested.clone()).unwrap();
        globals.raw_set("new_function", function.clone()).unwrap();
        restore(&lua, snapshot, old).unwrap();
        assert_eq!(
            globals.raw_get::<Table>("new_table").unwrap().to_pointer(),
            nested.to_pointer()
        );
        assert_eq!(
            globals
                .raw_get::<Function>("new_function")
                .unwrap()
                .to_pointer(),
            function.to_pointer()
        );
        globals.raw_remove("new_table").unwrap();
        globals.raw_remove("new_function").unwrap();
        assert_eq!(fingerprint(&globals), expected);
    }
}

#[test]
fn lua_capture_scalar_restore_validates_all_keys_before_mutation() {
    for scalar in [false, true] {
        let lua = Lua::new();
        let globals = lua.create_table().unwrap();
        lua.set_globals(globals.clone()).unwrap();
        let invalid = lua.create_string([0xff]).unwrap();
        globals.raw_set("temporary", 42).unwrap();
        globals
            .raw_set(
                invalid,
                if scalar {
                    Value::Integer(1)
                } else {
                    Value::Table(lua.create_table().unwrap())
                },
            )
            .unwrap();
        let expected = fingerprint(&globals);
        let mut errors = Vec::new();
        for old in [true, false] {
            errors.push(
                restore(&lua, vec![("saved".into(), Value::Integer(9))], old)
                    .unwrap_err()
                    .to_string(),
            );
            assert_eq!(fingerprint(&globals), expected);
        }
        assert_eq!(errors[0], errors[1]);
    }
}

#[test]
fn lua_capture_scalar_restore_preserves_metamethod_order_and_partial_errors() {
    for fail in [false, true] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let globals = lua.create_table().unwrap();
            lua.set_globals(globals.clone()).unwrap();
            globals.raw_set("temporary", 17).unwrap();
            let trace = Rc::new(RefCell::new(Vec::new()));
            let captured = trace.clone();
            let mt = lua.create_table().unwrap();
            mt.raw_set(
                "__newindex",
                lua.create_function(move |_, (table, key, value): (Table, String, Value)| {
                    captured.borrow_mut().push(key.clone());
                    if fail && key == "second" {
                        return Err(mlua::Error::runtime("restore failed"));
                    }
                    table.raw_set(key, value)
                })
                .unwrap(),
            )
            .unwrap();
            globals.set_metatable(Some(mt)).unwrap();
            let snapshot = vec![
                ("first".into(), Value::Integer(1)),
                ("second".into(), Value::Integer(2)),
                ("last".into(), Value::Integer(3)),
            ];
            let result = restore(&lua, snapshot, old);
            assert_eq!(result.is_err(), fail);
            assert!(matches!(
                globals.raw_get::<Value>("temporary").unwrap(),
                Value::Nil
            ));
            outcomes.push((
                result.err().map(|e| e.to_string()),
                trace.borrow().clone(),
                fingerprint(&globals),
            ));
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[test]
fn lua_capture_scalar_restore_empty_globals_have_no_churn() {
    let lua = Lua::new();
    lua.set_globals(lua.create_table().unwrap()).unwrap();
    restore_scalar_globals(&lua, Vec::new()).unwrap();
    crate::perf::assert_no_churn(|| restore_scalar_globals(&lua, Vec::new()).unwrap());
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_capture_bench_scalar_restore() {
    for (saved, added, key_len) in [
        (0, 0, 8),
        (16, 0, 8),
        (16, 8, 8),
        (16, 16, 8),
        (16, 17, 8),
        (64, 64, 8),
        (64, 512, 8),
        (64, 64, 256),
    ] {
        let lua = Lua::new();
        let globals = lua.create_table().unwrap();
        lua.set_globals(globals.clone()).unwrap();
        let snapshot: Vec<_> = (0..saved)
            .map(|i| (format!("saved_{i}"), Value::Integer(i as i64)))
            .collect();
        for (key, value) in &snapshot {
            globals.raw_set(key.as_str(), value.clone()).unwrap();
        }
        let keys: Vec<_> = (0..added)
            .map(|i| {
                lua.create_string(format!("temporary_{i}_{}", "x".repeat(key_len)))
                    .unwrap()
            })
            .collect();
        let expected = fingerprint(&globals);
        let roundtrip = |old| {
            for key in &keys {
                globals.raw_set(key, 42).unwrap();
            }
            restore(&lua, snapshot.clone(), old).unwrap();
        };
        for old in [true, false] {
            roundtrip(old);
            assert_eq!(fingerprint(&globals), expected);
        }
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "scalar_{saved}_{added}_{key_len}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                (saved + added).max(1),
                || roundtrip(black_box(old)),
            );
        }
    }
}
