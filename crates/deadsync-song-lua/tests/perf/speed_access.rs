use super::*;
use std::hint::black_box;

#[path = "speed_access_baseline.rs"]
mod baseline;

fn set(owner: &Table, key: &str, value: Option<f32>, old: bool) {
    if old {
        baseline::set_player_speedmod(owner, key, value).unwrap();
    } else {
        set_player_speedmod(owner, key, value).unwrap();
    }
}

fn snapshot(owner: &Table) -> Vec<(String, String)> {
    let mut values: Vec<_> = owner
        .pairs::<String, Value>()
        .map(|pair| {
            let (key, value) = pair.unwrap();
            let text = match value {
                Value::Number(n) => format!("n:{:x}", n.to_bits()),
                value => format!("{value:?}"),
            };
            (key, text)
        })
        .filter(|(key, _)| key.starts_with("__songlua_"))
        .collect();
    values.sort();
    values
}

#[test]
fn lua_access_speed_fields_preserve_switches_clear_and_unknown_keys() {
    let lua = Lua::new();
    let old = lua.create_table().unwrap();
    let new = lua.create_table().unwrap();
    let mt = lua
        .load("return {__newindex=function() error('must use raw writes') end}")
        .eval::<Table>()
        .unwrap();
    old.set_metatable(Some(mt.clone())).unwrap();
    new.set_metatable(Some(mt)).unwrap();
    let long = "custom".repeat(100);
    for key in [
        "xmod",
        "cmod",
        "mmod",
        "amod",
        "camod",
        "XMod",
        "",
        "éあ",
        long.as_str(),
    ] {
        for value in [
            Some(-0.0),
            Some(1.5),
            None,
            Some(f32::INFINITY),
            Some(f32::NAN),
            Some(-f32::INFINITY),
        ] {
            set(&old, key, value, true);
            set(&new, key, value, false);
            assert_eq!(snapshot(&new), snapshot(&old));
        }
    }
}

fn install(lua: &Lua, owner: &Table, name: &str, old: bool) -> Function {
    if old {
        baseline::install_speedmod_state_method(lua, owner, name, Value::Number(1.25)).unwrap();
    } else {
        install_speedmod_state_method(lua, owner, name, Value::Number(1.25)).unwrap();
    }
    owner.get(name).unwrap()
}

fn returned(value: mlua::Result<Value>, owner: &Table) -> String {
    match value {
        Ok(Value::Table(t)) => {
            assert_eq!(t.to_pointer(), owner.to_pointer());
            "owner".into()
        }
        Ok(Value::Number(n)) => format!("n:{:x}", n.to_bits()),
        value => format!("{value:?}"),
    }
}

#[test]
fn lua_access_speed_methods_preserve_defaults_arguments_getters_and_errors() {
    let lua = Lua::new();
    for name in ["XMod", "CMod", "MMod", "AMod", "CAMod", "UnusualéMethod"] {
        let old = lua.create_table().unwrap();
        let new = lua.create_table().unwrap();
        let methods = [
            install(&lua, &old, name, true),
            install(&lua, &new, name, false),
        ];
        let owners = [old, new];
        let get = |index: usize| {
            returned(
                methods[index].call((owners[index].clone(),)),
                &owners[index],
            )
        };
        assert_eq!(get(0), get(1));
        for value in [
            Value::Number(2.5),
            Value::Nil,
            Value::String(lua.create_string(" NaN ").unwrap()),
            Value::Boolean(false),
            Value::Table(lua.create_table().unwrap()),
            Value::String(lua.create_string([0xff]).unwrap()),
        ] {
            let a = returned(
                methods[0].call((owners[0].clone(), value.clone())),
                &owners[0],
            );
            let b = returned(
                methods[1].call((owners[1].clone(), value.clone())),
                &owners[1],
            );
            assert_eq!(a, b);
            assert_eq!(get(0), get(1));
            assert_eq!(snapshot(&owners[0]), snapshot(&owners[1]));
        }
        for active in [
            Value::Nil,
            Value::Integer(1),
            Value::String(lua.create_string("none").unwrap()),
            Value::String(lua.create_string([0xff]).unwrap()),
            Value::Table(lua.create_table().unwrap()),
        ] {
            for owner in &owners {
                owner
                    .raw_set("__songlua_speedmod_active", active.clone())
                    .unwrap();
            }
            assert_eq!(get(0), get(1));
        }
    }
}

#[test]
fn lua_access_speed_field_writes_have_no_churn_and_methods_keep_argument_budget() {
    let lua = Lua::new();
    let owner = lua.create_table().unwrap();
    let method = install(&lua, &owner, "XMod", false);
    for key in ["xmod", "cmod", "mmod", "amod", "camod"] {
        set(&owner, key, Some(1.0), false);
    }
    method.call::<Value>((owner.clone(), 2.0)).unwrap();
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        for key in ["xmod", "cmod", "mmod", "amod", "camod"] {
            set(&owner, black_box(key), Some(2.5), false);
        }
    });
    // The existing MultiValue callback owns its two-element argument list.
    crate::perf::assert_churn_budget(1, 2 * std::mem::size_of::<Value>(), || {
        black_box(method.call::<Value>((owner.clone(), 3.0)).unwrap());
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_access_bench_speed() {
    for (label, key, clear) in [
        ("common", "xmod", false),
        ("camod", "camod", false),
        ("clear", "cmod", true),
        ("unknown", "custom", false),
    ] {
        let lua = Lua::new();
        let owner = lua.create_table().unwrap();
        let value = if clear { None } else { Some(2.5) };
        set(&owner, key, value, true);
        let expected = snapshot(&owner);
        set(&owner, key, value, false);
        assert_eq!(snapshot(&owner), expected);
        lua.gc_stop();
        for old in order() {
            crate::perf::measure_sampled(
                &format!("speed_write_{label}/{}", if old { "old" } else { "new" }),
                128,
                64,
                || {
                    for _ in 0..64 {
                        set(black_box(&owner), black_box(key), black_box(value), old);
                    }
                    black_box(&owner);
                },
            );
        }
    }
    for name in ["XMod", "UnusualMethod"] {
        for read in [false, true] {
            let lua = Lua::new();
            let owners = [lua.create_table().unwrap(), lua.create_table().unwrap()];
            let methods = [
                install(&lua, &owners[0], name, true),
                install(&lua, &owners[1], name, false),
            ];
            for index in 0..2 {
                methods[index]
                    .call::<Value>((owners[index].clone(), 2.5))
                    .unwrap();
            }
            assert_eq!(snapshot(&owners[0]), snapshot(&owners[1]));
            lua.gc_stop();
            for old in order() {
                let index = usize::from(!old);
                crate::perf::measure_sampled(
                    &format!(
                        "speed_method_{name}_read_{read}/{}",
                        if old { "old" } else { "new" }
                    ),
                    128,
                    64,
                    || {
                        for _ in 0..64 {
                            black_box(if read {
                                methods[index]
                                    .call::<Value>((owners[index].clone(),))
                                    .unwrap()
                            } else {
                                methods[index]
                                    .call::<Value>((owners[index].clone(), black_box(2.5)))
                                    .unwrap()
                            });
                        }
                    },
                );
            }
        }
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}
