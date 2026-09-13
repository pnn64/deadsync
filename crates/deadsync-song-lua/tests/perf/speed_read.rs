use super::*;
use std::hint::black_box;

#[path = "speed_read_baseline.rs"]
mod baseline;

fn install(lua: &Lua, owner: &Table, name: &str, old: bool) -> Function {
    if old {
        baseline::install_speedmod_state_method(lua, owner, name, Value::Number(1.25)).unwrap();
    } else {
        install_speedmod_state_method(lua, owner, name, Value::Number(1.25)).unwrap();
    }
    owner.get(name).unwrap()
}

fn outcome(result: mlua::Result<Value>) -> String {
    match result {
        Ok(Value::Number(n)) => format!("number:{:x}", n.to_bits()),
        Ok(value) => format!("{value:?}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn speed_read_preserves_defaults_raw_lookup_coercion_and_errors() {
    let lua = Lua::new();
    let long = "unknown".repeat(100);
    for name in ["XMod", "CMod", "MMod", "AMod", "CAMod", "UnusualMethod"] {
        let owner = lua.create_table().unwrap();
        let old = install(&lua, &owner, name, true);
        let new = install(&lua, &owner, name, false);
        let key = name.to_ascii_lowercase();
        let value_key = format!("__songlua_speedmod_{key}");
        owner
            .set_metatable(Some(
                lua.load("return {__index=function() error('raw lookup required') end}")
                    .eval()
                    .unwrap(),
            ))
            .unwrap();
        for active in [
            Value::Nil,
            Value::String(lua.create_string(&key).unwrap()),
            Value::String(lua.create_string("none").unwrap()),
            Value::String(lua.create_string("xmod\0").unwrap()),
            Value::String(lua.create_string("éあ").unwrap()),
            Value::String(lua.create_string(&long).unwrap()),
            Value::String(lua.create_string([0xff]).unwrap()),
            Value::Integer(123),
            Value::Number(-0.0),
            Value::Boolean(false),
            Value::Table(lua.create_table().unwrap()),
        ] {
            owner.raw_set("__songlua_speedmod_active", active).unwrap();
            for value in [
                Value::Nil,
                Value::Number(-0.0),
                Value::Number(2.5),
                Value::Number(f64::NAN),
                Value::String(lua.create_string("3.5").unwrap()),
                Value::Boolean(true),
            ] {
                owner.raw_set(value_key.as_str(), value).unwrap();
                for receiver in [false, true] {
                    let call = |function: &Function| {
                        if receiver {
                            function.call((owner.clone(),))
                        } else {
                            function.call(())
                        }
                    };
                    assert_eq!(
                        outcome(call(&new)),
                        outcome(call(&old)),
                        "name={name}, receiver={receiver}"
                    );
                }
            }
        }
    }
}

#[test]
fn speed_read_observes_switches_clears_and_live_field_changes() {
    let lua = Lua::new();
    let owner = lua.create_table().unwrap();
    let x = install(&lua, &owner, "XMod", false);
    let c = install(&lua, &owner, "CMod", false);
    assert_eq!(x.call::<f32>(()).unwrap(), 1.25);
    x.call::<Value>((owner.clone(), 2.5)).unwrap();
    assert_eq!(x.call::<f32>(()).unwrap(), 2.5);
    assert!(matches!(c.call::<Value>(()).unwrap(), Value::Nil));
    c.call::<Value>((owner.clone(), 600.0)).unwrap();
    assert!(matches!(x.call::<Value>(()).unwrap(), Value::Nil));
    assert_eq!(c.call::<f32>(()).unwrap(), 600.0);
    owner.raw_set("__songlua_speedmod_cmod", 450.0).unwrap();
    assert_eq!(c.call::<f32>(()).unwrap(), 450.0);
    c.call::<Value>((owner.clone(), Value::Nil)).unwrap();
    assert!(matches!(c.call::<Value>(()).unwrap(), Value::Nil));
}

#[test]
fn speed_read_has_no_string_churn_and_retains_receiver_argument_budget() {
    let lua = Lua::new();
    let owner = lua.create_table().unwrap();
    let method = install(&lua, &owner, "XMod", false);
    owner.raw_set("__songlua_speedmod_xmod", 2.5).unwrap();
    for active in ["xmod", "cmod", "none", ""] {
        owner.raw_set("__songlua_speedmod_active", active).unwrap();
        method.call::<Value>(()).unwrap();
        method.call::<Value>((owner.clone(),)).unwrap();
        lua.gc_stop();
        crate::perf::assert_no_churn(|| {
            for _ in 0..64 {
                black_box(method.call::<Value>(()).unwrap());
            }
        });
        crate::perf::assert_churn_budget(1, std::mem::size_of::<Value>(), || {
            black_box(method.call::<Value>((owner.clone(),)).unwrap());
        });
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn speed_read_bench() {
    for index in [0, 1] {
        let lua = Lua::new();
        lua.gc_stop();
        crate::perf::measure_sampled(
            &format!("speed_install/{}", if index == 0 { "old" } else { "new" }),
            32,
            1,
            || {
                let owner = lua.create_table().unwrap();
                black_box(install(&lua, &owner, "XMod", index == 0));
            },
        );
    }
    let lua = Lua::new();
    let owner = lua.create_table().unwrap();
    let methods = [
        install(&lua, &owner, "XMod", true),
        install(&lua, &owner, "XMod", false),
    ];
    owner.raw_set("__songlua_speedmod_xmod", 2.5).unwrap();
    for (label, active) in [
        ("default", None),
        ("active", Some("xmod")),
        ("inactive", Some("cmod")),
        ("cleared", Some("none")),
    ] {
        owner.raw_set("__songlua_speedmod_active", active).unwrap();
        assert_eq!(outcome(methods[0].call(())), outcome(methods[1].call(())));
        lua.gc_stop();
        for receiver in [false, true] {
            let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
                [1, 0]
            } else {
                [0, 1]
            };
            for index in order {
                let method = &methods[index];
                crate::perf::measure_sampled(
                    &format!(
                        "speed_read_{label}_receiver_{receiver}/{}",
                        if index == 0 { "old" } else { "new" }
                    ),
                    256,
                    128,
                    || {
                        for _ in 0..128 {
                            black_box(
                                if receiver {
                                    method.call::<Value>((owner.clone(),))
                                } else {
                                    method.call::<Value>(())
                                }
                                .unwrap(),
                            );
                        }
                    },
                );
            }
        }
    }
}
