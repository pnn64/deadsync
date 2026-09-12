use super::*;
use std::hint::black_box;

#[path = "global_snapshot_baseline.rs"]
mod baseline;

fn snapshot(lua: &Lua, old: bool) -> mlua::Result<Vec<(String, Value)>> {
    if old {
        baseline::snapshot_scalar_globals(lua)
    } else {
        snapshot_scalar_globals(lua)
    }
}

fn fingerprint(values: Vec<(String, Value)>) -> Vec<(String, String)> {
    values
        .into_iter()
        .map(|(key, value)| {
            (
                key,
                match value {
                    Value::String(s) => format!("bytes:{:?}", s.as_bytes().as_ref()),
                    Value::Number(n) => format!("number:{:x}", n.to_bits()),
                    v => format!("{v:?}"),
                },
            )
        })
        .collect()
}

fn fixture(lua: &Lua, ignored: usize, scalars: usize) {
    let globals = lua.globals();
    let function = lua.create_function(|_, ()| Ok(())).unwrap();
    let table = lua.create_table().unwrap();
    for i in 0..ignored {
        globals
            .raw_set(
                format!("ignored_{i}"),
                if i % 2 == 0 {
                    Value::Function(function.clone())
                } else {
                    Value::Table(table.clone())
                },
            )
            .unwrap();
    }
    for i in 0..scalars {
        globals.raw_set(format!("scalar_{i}"), i).unwrap();
    }
}

#[test]
fn lua_command_global_snapshot_preserves_order_values_and_raw_filtering() {
    let lua = Lua::new();
    lua.globals().clear().unwrap();
    fixture(&lua, 256, 32);
    let globals = lua.globals();
    for (key, value) in [
        ("flag", Value::Boolean(false)),
        ("negzero", Value::Number(-0.0)),
        ("nan", Value::Number(f64::NAN)),
        ("inf", Value::Number(f64::INFINITY)),
        (
            "text",
            Value::String(lua.create_string([255, 0, 254]).unwrap()),
        ),
    ] {
        globals.raw_set(key, value).unwrap();
    }
    globals.raw_set(true, 3).unwrap();
    globals.raw_set(3, 4).unwrap();
    globals.raw_set(&globals, 5).unwrap();
    let mt = lua.create_table().unwrap();
    mt.raw_set(
        "__pairs",
        lua.create_function(|_, ()| -> mlua::Result<()> {
            Err(mlua::Error::runtime("pairs trap"))
        })
        .unwrap(),
    )
    .unwrap();
    globals.set_metatable(Some(mt)).unwrap();
    assert_eq!(
        fingerprint(snapshot(&lua, true).unwrap()),
        fingerprint(snapshot(&lua, false).unwrap())
    );
    assert_eq!(snapshot(&lua, false).unwrap().len(), 37);
}

#[test]
fn lua_command_global_snapshot_preserves_invalid_key_error_filtering() {
    let lua = Lua::new();
    lua.globals().clear().unwrap();
    let bad = lua.create_string([255, 0]).unwrap();
    let globals = lua.globals();
    globals.raw_set(&bad, lua.create_table().unwrap()).unwrap();
    assert!(snapshot(&lua, true).unwrap().is_empty());
    assert!(snapshot(&lua, false).unwrap().is_empty());
    globals.raw_set(&bad, 42).unwrap();
    assert_eq!(
        snapshot(&lua, true).unwrap_err().to_string(),
        snapshot(&lua, false).unwrap_err().to_string()
    );
    assert_eq!(globals.raw_get::<i32>(&bad).unwrap(), 42);
}

#[test]
fn lua_command_global_snapshot_preserves_command_capture_and_scalar_restoration() {
    let mut outcomes = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        fixture(&lua, 128, 16);
        let actor = lua.create_table().unwrap();
        install_actor_transform_methods(&lua, &actor).unwrap();
        lua.globals().raw_set("counter", 7).unwrap();
        actor.raw_set("TestCommand",lua.load("return function(self) counter=counter+1; added='temporary'; self:x(counter) end").eval::<Function>().unwrap()).unwrap();
        let result = if old {
            baseline::capture_actor_command_preserving_state(&lua, &actor, "TestCommand")
        } else {
            capture_actor_command_preserving_state(&lua, &actor, "TestCommand")
        }
        .unwrap();
        assert!(!result.is_empty());
        assert_eq!(lua.globals().get::<i32>("counter").unwrap(), 7);
        assert!(lua.globals().raw_get::<Value>("added").unwrap().is_nil());
        outcomes.push(result);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn lua_command_global_snapshot_non_scalar_globals_have_no_churn() {
    let lua = Lua::new();
    lua.globals().clear().unwrap();
    fixture(&lua, 512, 0);
    snapshot(&lua, false).unwrap();
    crate::perf::assert_no_churn(|| assert!(snapshot(&lua, false).unwrap().is_empty()));
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_command_bench_global_snapshot() {
    for (ignored, scalars) in [(0, 0), (0, 64), (64, 0), (256, 0), (256, 8), (512, 32)] {
        let lua = Lua::new();
        lua.globals().clear().unwrap();
        fixture(&lua, ignored, scalars);
        assert_eq!(
            fingerprint(snapshot(&lua, true).unwrap()),
            fingerprint(snapshot(&lua, false).unwrap())
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
                    "global_snapshot_{ignored}_{scalars}/{}",
                    if old { "old" } else { "new" }
                ),
                256,
                (ignored + scalars).max(1),
                || {
                    drop(black_box(
                        snapshot(black_box(&lua), black_box(old)).unwrap(),
                    ));
                },
            );
        }
    }
    for ignored in [0, 256] {
        let lua = Lua::new();
        fixture(&lua, ignored, 0);
        let actor = lua.create_table().unwrap();
        actor
            .raw_set(
                "TestCommand",
                lua.create_function(|_, _: Table| Ok(())).unwrap(),
            )
            .unwrap();
        let capture = |old| {
            if old {
                baseline::capture_actor_command_preserving_state(&lua, &actor, "TestCommand")
            } else {
                capture_actor_command_preserving_state(&lua, &actor, "TestCommand")
            }
        };
        assert_eq!(capture(true).unwrap(), capture(false).unwrap());
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
                    "global_capture_{ignored}/{}",
                    if old { "old" } else { "new" }
                ),
                64,
                1,
                || {
                    drop(black_box(capture(black_box(old)).unwrap()));
                },
            );
        }
    }
}
