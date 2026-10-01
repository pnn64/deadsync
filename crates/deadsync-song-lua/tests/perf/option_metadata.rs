use super::*;
use std::hint::black_box;

#[path = "option_metadata_baseline.rs"]
mod baseline;

fn method(lua: &Lua, owner: &Table, name: &str, old: bool) -> Function {
    if old {
        baseline::create_player_option_method(lua, owner, name)
    } else {
        create_player_option_method(lua, owner, name)
    }
    .unwrap()
}

fn fingerprint(value: Value) -> String {
    match value {
        Value::Number(n) => format!("number:{:x}", n.to_bits()),
        Value::String(s) => format!("string:{:?}", s.as_bytes().as_ref()),
        v => format!("{v:?}"),
    }
}

#[test]
fn option_metadata_matches_parent_for_unchanged_options() {
    let lua = Lua::new();
    let foreign = lua.create_table().unwrap();
    let callback = lua.create_function(|_, ()| Ok(())).unwrap();
    let mut values = vec![
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(i64::MAX),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Table(foreign.clone()),
        Value::Function(callback),
    ];
    for bytes in [
        b"2.5".as_slice(),
        b" YES ",
        b"NaN",
        b"unknown",
        &[0xff, 0, 0xfe],
    ] {
        values.push(Value::String(lua.create_string(bytes).unwrap()));
    }
    let mut names: Vec<String> = SONG_LUA_PLAYER_OPTION_CAPABILITIES
        .iter()
        // These options follow native return and approach semantics. Source
        // tests and native probes cover their deliberate differences.
        .filter(|name| {
            !matches!(
                **name,
                "Incoming"
                    | "Space"
                    | "Hallway"
                    | "Distant"
                    | "Overhead"
                    | "Tilt"
                    | "Skew"
                    | "DrawSize"
                    | "DrawSizeBack"
                    | "ModTimerSetting"
                    | "ModTimerMult"
                    | "ModTimerOffset"
                    | "BumpyX"
                    | "BumpyXOffset"
                    | "BumpyXPeriod"
                    | "TanBumpy"
                    | "TanBumpyOffset"
                    | "TanBumpyPeriod"
                    | "TanBumpyX"
                    | "TanBumpyXOffset"
                    | "TanBumpyXPeriod"
                    | "DrunkZ"
                    | "DrunkZOffset"
                    | "DrunkZSpeed"
                    | "DrunkZPeriod"
                    | "TanDrunk"
                    | "TanDrunkOffset"
                    | "TanDrunkSpeed"
                    | "TanDrunkPeriod"
                    | "TanDrunkZ"
                    | "TanDrunkZOffset"
                    | "TanDrunkZSpeed"
                    | "TanDrunkZPeriod"
                    | "StealthType"
                    | "ZBuffer"
                    | "DizzyHolds"
                    | "Cosecant"
            )
        })
        .map(|name| name.to_string())
        .collect();
    names.extend([
        "Custom".repeat(16),
        "Mixed_\u{e9}\0Name".to_string(),
        "mIrRoR".to_string(),
    ]);
    for name in names {
        let key = name.to_ascii_lowercase();
        let owner = lua.create_table().unwrap();
        let old = method(&lua, &owner, &name, true);
        let new = method(&lua, &owner, &name, false);
        let state = player_option_state(&lua, &owner).unwrap();
        let speeds = player_option_speeds(&lua, &owner).unwrap();
        for value in &values {
            for count in [0, 1, 2, 3, 257] {
                let mut args = MultiValue::new();
                if count > 0 {
                    args.push_back(Value::Table(foreign.clone()));
                }
                if count > 1 {
                    args.push_back(value.clone());
                }
                if count > 2 {
                    args.push_back(Value::Number(0.5));
                    args.extend(std::iter::repeat_n(Value::Integer(99), count - 3));
                }
                let mut outcomes = Vec::new();
                for function in [&old, &new] {
                    state.raw_set(key.as_str(), Value::Nil).unwrap();
                    speeds.raw_set(key.as_str(), 0.75).unwrap();
                    let result = match function.call::<Value>(&args) {
                        Ok(Value::Table(t)) => {
                            format!("owner:{}", t.to_pointer() == owner.to_pointer())
                        }
                        Ok(v) => fingerprint(v),
                        Err(e) => e.to_string(),
                    };
                    outcomes.push((
                        result,
                        fingerprint(state.raw_get(key.as_str()).unwrap()),
                        fingerprint(speeds.raw_get(key.as_str()).unwrap()),
                    ));
                }
                assert_eq!(
                    outcomes[0], outcomes[1],
                    "{name}, count={count}, value={value:?}"
                );
            }
        }
    }
}

#[test]
fn option_metadata_keeps_live_replaced_tables_and_eager_error_order() {
    for name in ["Mini", "Mirror", "LifeSetting"] {
        for stage in [
            "none",
            "state_read",
            "state_write",
            "speed_read",
            "speed_write",
        ] {
            let mut outcomes = Vec::new();
            for old in [true, false] {
                let lua = Lua::new();
                let owner = lua.create_table().unwrap();
                let function = method(&lua, &owner, name, old);
                lua.globals().set("stage", stage).unwrap();
                lua.globals()
                    .set("name", name.to_ascii_lowercase())
                    .unwrap();
                lua.globals().set("owner", owner.clone()).unwrap();
                let state=lua.load(r#"seen='';return setmetatable({}, {
                    __index=function(_,k)seen=seen..'state_read;';if stage=='state_read' then error('state read failed') end;return 4.5 end,
                    __newindex=function(t,k,v)seen=seen..'state_write;';rawset(t,k,v);if stage=='state_write' then error('state write failed') end end})"#).set_name("option-errors").eval::<Table>().unwrap();
                let speeds=lua.load(r#"return setmetatable({}, {
                    __index=function(_,k)seen=seen..'speed_read;';if stage=='speed_read' then error('speed read failed') end;return 0.25 end,
                    __newindex=function(t,k,v)seen=seen..'speed_write;';rawset(t,k,v);if stage=='speed_write' then error('speed write failed') end end})"#).set_name("option-errors").eval::<Table>().unwrap();
                owner
                    .raw_set("__songlua_player_option_state", state.clone())
                    .unwrap();
                owner
                    .raw_set("__songlua_player_option_speeds", speeds.clone())
                    .unwrap();
                let read = function
                    .call::<Value>(&owner)
                    .map(fingerprint)
                    .map_err(|e| e.to_string());
                // An explicit approach still eagerly reads the prior speed.
                let written = function
                    .call::<Value>((&owner, 2.0, 0.5))
                    .map(|v| matches!(v,Value::Table(t) if t.to_pointer()==owner.to_pointer()))
                    .map_err(|e| e.to_string());
                outcomes.push((
                    read,
                    written,
                    lua.globals().get::<String>("seen").unwrap(),
                    fingerprint(state.raw_get(name.to_ascii_lowercase()).unwrap()),
                    fingerprint(speeds.raw_get(name.to_ascii_lowercase()).unwrap()),
                ));
                let replacement = lua.create_table().unwrap();
                replacement
                    .raw_set(name.to_ascii_lowercase(), "live")
                    .unwrap();
                owner
                    .raw_set("__songlua_player_option_state", replacement)
                    .unwrap();
                assert_eq!(function.call::<String>(&owner).unwrap(), "live");
                let count = crate::runtime::song_lua_side_effect_count(&lua).unwrap();
                assert_eq!(count, 0);
            }
            assert_eq!(outcomes[0], outcomes[1], "{name}, stage={stage}");
            if stage == "speed_read" {
                assert!(outcomes[0].1.is_err());
                assert!(outcomes[0].2.contains("state_write;speed_read;"));
            }
        }
    }
    let lua = Lua::new();
    let owner = lua.create_table().unwrap();
    let old = method(&lua, &owner, "Mini", true);
    let new = method(&lua, &owner, "Mini", false);
    let speeds = player_option_speeds(&lua, &owner).unwrap();
    speeds.raw_set("mini", true).unwrap();
    let read = |f: &Function| {
        f.call::<Value>((&owner, 1.5, 0.5))
            .map(fingerprint)
            .map_err(|e| e.to_string())
    };
    assert_eq!(read(&old), read(&new));
    assert!(read(&new).is_err());
    assert_eq!(
        player_option_state(&lua, &owner)
            .unwrap()
            .raw_get::<f32>("mini")
            .unwrap(),
        1.5
    );
    // Argument conversion errors happen before creating the state table.
    for old in [true, false] {
        let owner = lua.create_table().unwrap();
        let function = method(&lua, &owner, "Mini", old);
        assert!(function.call::<Value>((&owner, 2.0, true)).is_err());
        assert!(matches!(
            owner
                .raw_get::<Value>("__songlua_player_option_state")
                .unwrap(),
            Value::Nil
        ));
    }
}

#[test]
fn option_metadata_has_zero_warm_churn_even_for_long_field_names() {
    let lua = Lua::new();
    lua.gc_stop();
    let owner = lua.create_table().unwrap();
    for name in [
        "Mini".to_string(),
        "Mirror".to_string(),
        "LifeSetting".to_string(),
        "Custom".repeat(16),
    ] {
        let old = method(&lua, &owner, &name, true);
        let new = method(&lua, &owner, &name, false);
        for _ in 0..8 {
            old.call::<Value>((&owner, 1.5, 0.25)).unwrap();
            new.call::<Value>((&owner, 1.5, 0.25)).unwrap();
        }
        crate::perf::assert_no_churn(|| {
            for _ in 0..64 {
                black_box(new.call::<Value>((&owner, 1.5, 0.25)).unwrap());
                black_box(new.call::<Value>(&owner).unwrap());
            }
        });
        // Parent long keys allocate, but remain uncollected while GC is stopped.
        // The paired benchmark records those allocations without claiming a
        // strict free-count reduction; the new path must have no churn at all.
    }
}

#[test]
#[ignore = "manual release comparison against 0.5.1634"]
fn retained_values_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (label, name) in [
        ("number", "Mini".to_string()),
        ("boolean", "Mirror".to_string()),
        ("string", "LifeSetting".to_string()),
        ("long_key", "Custom".repeat(16)),
    ] {
        for action in ["get", "set", "default", "install"] {
            for old in order {
                let mode = if old { "old" } else { "new" };
                if action == "install" {
                    crate::perf::measure_sampled_with_setup(
                        &format!("option/{label}/{action}/{mode}"),
                        96,
                        1,
                        || {
                            let lua = Lua::new();
                            lua.gc_stop();
                            let owner = lua.create_table().unwrap();
                            (lua, owner)
                        },
                        |(lua, owner)| {
                            black_box(method(lua, owner, &name, old));
                        },
                    );
                    continue;
                }
                let lua = Lua::new();
                lua.gc_stop();
                let owner = lua.create_table().unwrap();
                let function = method(&lua, &owner, &name, old);
                let value = if label == "string" {
                    Value::String(lua.create_string("LifeType_Battery").unwrap())
                } else if label == "boolean" {
                    Value::Boolean(true)
                } else {
                    Value::Number(1.5)
                };
                if action != "default" {
                    function.call::<Value>((&owner, &value, 0.25)).unwrap();
                }
                crate::perf::measure_sampled(
                    &format!("option/{label}/{action}/{mode}"),
                    256,
                    64,
                    || {
                        for _ in 0..64 {
                            black_box(
                                if action == "set" {
                                    function.call::<Value>((&owner, &value, 0.25))
                                } else {
                                    function.call::<Value>(&owner)
                                }
                                .unwrap(),
                            );
                        }
                    },
                );
            }
        }
    }
    for old in order {
        let lua = Lua::new();
        lua.gc_stop();
        let owner = lua.create_table().unwrap();
        owner
            .set("Mini", method(&lua, &owner, "Mini", old))
            .unwrap();
        owner
            .set("Mirror", method(&lua, &owner, "Mirror", old))
            .unwrap();
        lua.globals().set("owner", owner).unwrap();
        let run=lua.load("return function()for i=1,64 do owner:Mini(1.5,0.25);owner:Mirror(true,0.25);local a,b=owner:Mini(),owner:Mirror() end end").eval::<Function>().unwrap();
        crate::perf::measure_sampled(
            &format!("option/lua_loop/{}", if old { "old" } else { "new" }),
            256,
            256,
            || run.call::<()>(()).unwrap(),
        );
    }
}
