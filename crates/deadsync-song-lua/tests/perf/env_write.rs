use super::*;
use std::hint::black_box;

#[path = "env_write_baseline.rs"]
mod baseline;

const KEYS: [&str; 6] = [
    "prefix_globals",
    "mods",
    "mod_time",
    "mods_ease",
    "mod_perframes",
    "mod_actions",
];

fn proxy(lua: &Lua, target: Table, old: bool) -> Table {
    if old {
        baseline::create_chunk_env_proxy(lua, target)
    } else {
        create_chunk_env_proxy(lua, target)
    }
    .unwrap()
}

#[test]
fn env_writes_match_parent_exact_keys_value_identity_and_deletion() {
    let lua = Lua::new();
    let reference = lua.create_table().unwrap();
    let callback = lua.create_function(|_, ()| Ok(())).unwrap();
    let thread = lua.create_thread(callback.clone()).unwrap();
    let mut keys: Vec<Value> = KEYS
        .iter()
        .map(|k| Value::String(lua.create_string(k).unwrap()))
        .collect();
    for bytes in [b"MODS".as_slice(), b"mods\0", b"other", &[0xff, 0, 0xfe]] {
        keys.push(Value::String(lua.create_string(bytes).unwrap()));
    }
    keys.push(Value::String(
        lua.create_string("mods".repeat(128)).unwrap(),
    ));
    keys.extend([
        Value::Integer(17),
        Value::Boolean(true),
        Value::Number(1.25),
        Value::Table(reference.clone()),
        Value::Function(callback.clone()),
        Value::Thread(thread.clone()),
        Value::Nil,
        Value::Number(f64::NAN),
    ]);
    let values = [
        Value::Nil,
        Value::Boolean(false),
        Value::Number(-0.0),
        Value::Integer(42),
        Value::String(lua.create_string([0xff, 0]).unwrap()),
        Value::Table(reference),
        Value::Function(callback),
        Value::Thread(thread),
    ];
    for (index, key) in keys.iter().enumerate() {
        for value in &values {
            let mut outcomes = Vec::new();
            for old in [true, false] {
                for key in KEYS {
                    lua.globals().raw_set(key, Value::Nil).unwrap();
                }
                let target = lua.create_table().unwrap();
                let env = proxy(&lua, target.clone(), old);
                let result = env.set(key, value).map_err(|e| e.to_string());
                // A nil/NaN lookup has its own error, so compare both outcomes.
                let stored = target.raw_get::<Value>(key).map_err(|e| e.to_string());
                let globals = KEYS.map(|key| lua.globals().raw_get::<Value>(key).unwrap());
                if index < 6 && result.is_ok() {
                    assert_eq!(globals[index], *value);
                } else {
                    assert!(globals.iter().all(|v| matches!(v, Value::Nil)));
                }
                outcomes.push((result, stored, globals));
            }
            assert_eq!(outcomes[0], outcomes[1], "key={key:?}, value={value:?}");
        }
    }
}

#[test]
fn env_writes_preserve_dynamic_target_metatables_and_partial_error_order() {
    for stage in ["none", "target", "global", "retarget"] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            lua.globals().set("stage", stage).unwrap();
            let first = lua.create_table().unwrap();
            let second = lua.create_table().unwrap();
            let env = proxy(&lua, first.clone(), old);
            lua.globals().set("env", env.clone()).unwrap();
            lua.globals().set("second", second.clone()).unwrap();
            let mt = lua
                .load(
                    r#"seen='';return {__newindex=function(t,k,v)
                seen=seen..'target:'..k..';';rawset(t,k,v)
                if stage=='target' then error('target failed after write') end
                if stage=='retarget' then rawset(env,'__songlua_env_target',second) end
            end}"#,
                )
                .set_name("env-errors")
                .eval::<Table>()
                .unwrap();
            first.set_metatable(Some(mt)).unwrap();
            let gmt = lua
                .load(
                    r#"return {__newindex=function(t,k,v)
                seen=seen..'global:'..k..';';rawset(t,k,v)
                if stage=='global' then error('global failed after write') end
            end}"#,
                )
                .set_name("env-errors")
                .eval::<Table>()
                .unwrap();
            let globals = lua.globals();
            globals.set_metatable(Some(gmt)).unwrap();
            let result = env.set("mods", 23).map_err(|e| e.to_string());
            globals.set_metatable(None).unwrap();
            let first_value = first.raw_get::<Option<i64>>("mods").unwrap();
            let global_value = globals.raw_get::<Option<i64>>("mods").unwrap();
            let events = globals.get::<String>("seen").unwrap();
            env.raw_set("__songlua_env_target", second.clone()).unwrap();
            env.set("mods", 31).unwrap();
            assert_eq!(first.raw_get::<Option<i64>>("mods").unwrap(), first_value);
            assert_eq!(second.raw_get::<i64>("mods").unwrap(), 31);
            assert_eq!(globals.get::<i64>("mods").unwrap(), 31);
            globals.set("fallback", 77).unwrap();
            assert_eq!(env.get::<i64>("fallback").unwrap(), 77);
            second.set("fallback", false).unwrap();
            assert!(!env.get::<bool>("fallback").unwrap());
            env.raw_set("__songlua_env_target", true).unwrap();
            let wrong_target = env.set("mods", 99).unwrap_err().to_string();
            assert_eq!(globals.get::<i64>("mods").unwrap(), 31);
            outcomes.push((result, first_value, global_value, events, wrong_target));
        }
        assert_eq!(outcomes[0], outcomes[1], "stage={stage}");
        assert_eq!(outcomes[0].1, Some(23));
        if stage == "target" {
            assert_eq!(outcomes[0].2, None);
            assert!(!outcomes[0].3.contains("global:"));
        } else {
            assert_eq!(outcomes[0].2, Some(23));
            assert!(outcomes[0].3.contains("global:mods;"));
        }
    }
}

#[test]
fn env_writes_have_zero_warm_churn_for_fixed_and_unrelated_keys() {
    let lua = Lua::new();
    lua.gc_stop();
    let reference = lua.create_table().unwrap();
    let old = proxy(&lua, lua.create_table().unwrap(), true);
    let new = proxy(&lua, lua.create_table().unwrap(), false);
    let keys = [
        "mods",
        "prefix_globals",
        "unrelated",
        "long unrelated key ".repeat(256).as_str(),
    ]
    .map(|k| lua.create_string(k).unwrap());
    for key in keys {
        for _ in 0..8 {
            old.set(&key, &reference).unwrap();
            new.set(&key, &reference).unwrap();
        }
        crate::perf::assert_no_churn(|| {
            for _ in 0..64 {
                new.set(&key, &reference).unwrap();
            }
        });
        crate::perf::assert_reduced_churn(
            || {
                for _ in 0..64 {
                    old.set(&key, &reference).unwrap();
                }
            },
            || {
                for _ in 0..64 {
                    new.set(&key, &reference).unwrap();
                }
            },
        );
    }
}

#[test]
#[ignore = "manual release comparison against 0.5.1633"]
fn call_transfer_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for label in [
        "fixed_numeric",
        "fixed_reference",
        "unrelated_numeric",
        "unrelated_reference",
        "long_key",
        "invalid_utf8",
        "numeric_key",
        "read_control",
        "lua_loop",
    ] {
        for old in order {
            let lua = Lua::new();
            lua.gc_stop();
            let target = lua.create_table().unwrap();
            let env = proxy(&lua, target.clone(), old);
            let reference = lua.create_table().unwrap();
            let key = match label {
                "long_key" => Value::String(lua.create_string("unrelated ".repeat(256)).unwrap()),
                "invalid_utf8" => Value::String(lua.create_string([0xff, 0, 0xfe]).unwrap()),
                "numeric_key" => Value::Integer(17),
                label if label.starts_with("fixed") => {
                    Value::String(lua.create_string("mods").unwrap())
                }
                _ => Value::String(lua.create_string("unrelated").unwrap()),
            };
            let value = if label.ends_with("reference") {
                Value::Table(reference)
            } else {
                Value::Integer(23)
            };
            env.set(&key, &value).unwrap();
            lua.globals().set("env", env.clone()).unwrap();
            let run = lua
                .load("return function() for i=1,64 do env.mods=i; env.unrelated=i end end")
                .eval::<Function>()
                .unwrap();
            crate::perf::measure_sampled(
                &format!("env/{label}/{}", if old { "old" } else { "new" }),
                256,
                if label == "lua_loop" { 128 } else { 64 },
                || {
                    if label == "lua_loop" {
                        run.call::<()>(()).unwrap();
                    } else if label == "read_control" {
                        for _ in 0..64 {
                            black_box(env.get::<Value>(&key).unwrap());
                        }
                    } else {
                        for _ in 0..64 {
                            env.set(&key, &value).unwrap();
                        }
                    }
                },
            );
        }
    }
    for prepared in [false, true] {
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "env/install/{}/{}",
                    if prepared { "prepared" } else { "fresh" },
                    if old { "old" } else { "new" }
                ),
                128,
                1,
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    let target = lua.create_table().unwrap();
                    if prepared {
                        for key in KEYS {
                            lua.globals().set(key, true).unwrap();
                        }
                    }
                    (lua, target)
                },
                |(lua, target)| {
                    black_box(proxy(lua, target.clone(), old));
                },
            );
        }
    }
}
