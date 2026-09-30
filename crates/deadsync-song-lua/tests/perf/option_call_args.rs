use super::*;
use mlua::FromLuaMulti;
use std::hint::black_box;

#[path = "option_call_args_baseline.rs"]
mod baseline;

fn speed(lua: &Lua, owner: &Table, name: &str, old: bool) -> Function {
    if old {
        baseline::install_speedmod_state_method(lua, owner, name, Value::Number(3.0)).unwrap();
    } else {
        install_speedmod_state_method(lua, owner, name, Value::Number(3.0)).unwrap();
    }
    owner.get(name).unwrap()
}

fn music(lua: &Lua, old: bool) -> (Table, Function) {
    let table = if old {
        baseline::create_song_options_table(lua, 1.25)
    } else {
        create_song_options_table(lua, 1.25)
    }
    .unwrap();
    let method = table.get("MusicRate").unwrap();
    (table, method)
}

fn fingerprint(value: Value) -> String {
    match value {
        Value::Number(v) => format!("number:{:x}", v.to_bits()),
        Value::String(v) => format!("string:{:?}", v.as_bytes().as_ref()),
        other => format!("{other:?}"),
    }
}

fn speed_outcome(
    lua: &Lua,
    args: &MultiValue,
    name: &str,
    old: bool,
    corrupt: bool,
) -> Vec<String> {
    let owner = lua.create_table().unwrap();
    owner.raw_set("__songlua_speedmod_active", "xmod").unwrap();
    owner.raw_set("__songlua_speedmod_xmod", 2.0).unwrap();
    let speeds = player_option_speeds(lua, &owner).unwrap();
    speeds
        .raw_set(
            "xmod",
            if corrupt {
                Value::Boolean(true)
            } else {
                Value::Number(0.75)
            },
        )
        .unwrap();
    let method = speed(lua, &owner, name, old);
    let mut out = vec![match method.call::<Value>(args) {
        Ok(Value::Table(result)) => format!("owner:{}", result.to_pointer() == owner.to_pointer()),
        Ok(value) => fingerprint(value),
        Err(error) => error.to_string(),
    }];
    for key in [
        "__songlua_speedmod_active",
        "__songlua_speedmod_xmod",
        "__songlua_speedmod_cmod",
        "__songlua_speedmod_camod",
        "__songlua_speedmod_weirdspeed",
    ] {
        out.push(fingerprint(owner.raw_get(key).unwrap()));
    }
    for key in ["xmod", "cmod", "mmod"] {
        out.push(fingerprint(speeds.raw_get(key).unwrap()));
    }
    out
}

#[test]
fn option_call_prefix_preserves_presence_and_first_stack_values() {
    let lua = Lua::new();
    let receiver = lua.create_table().unwrap();
    let callback = lua
        .create_function(|_, mut args: crate::method_args::MethodArgs<3>| {
            let first = args.take_method_arg(0);
            let second = args.take_method_arg(1);
            Ok((
                first.is_some(),
                first.unwrap_or(Value::Nil),
                second.is_some(),
                second.unwrap_or(Value::Nil),
            ))
        })
        .unwrap();
    for colon in [false, true] {
        for count in [0, 1, 2, 3, 8, 64, 257] {
            for first in [Value::Nil, Value::Integer(17)] {
                let mut args = MultiValue::new();
                if colon {
                    args.push_back(Value::Table(receiver.clone()));
                }
                for index in 0..count {
                    args.push_back(if index == 0 {
                        first.clone()
                    } else {
                        Value::Integer(index as i64)
                    });
                }
                let expected = (
                    method_arg(&args, 0).is_some(),
                    method_arg(&args, 0).cloned().unwrap_or(Value::Nil),
                    method_arg(&args, 1).is_some(),
                    method_arg(&args, 1).cloned().unwrap_or(Value::Nil),
                );
                assert_eq!(
                    callback.call::<(bool, Value, bool, Value)>(&args).unwrap(),
                    expected
                );
                let mut fallback =
                    crate::method_args::MethodArgs::<3>::from_lua_multi(args.clone(), &lua)
                        .unwrap();
                assert_eq!(fallback.take_method_arg(0), method_arg(&args, 0).cloned());
                assert_eq!(fallback.take_method_arg(1), method_arg(&args, 1).cloned());
            }
        }
    }
}

#[test]
fn option_calls_match_parent_coercions_aliases_clears_and_partial_errors() {
    let lua = Lua::new();
    let foreign = lua.create_table().unwrap();
    let callback = lua.create_function(|_, ()| Ok(())).unwrap();
    let mut values = vec![
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(i64::MAX),
        Value::Number(-2.5),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Table(foreign.clone()),
        Value::Function(callback.clone()),
        Value::Thread(lua.create_thread(callback).unwrap()),
    ];
    for bytes in [
        b" 2.5 ".as_slice(),
        b"NaN",
        b"inf",
        b"invalid",
        &[0xff, 0, 0xfe],
    ] {
        values.push(Value::String(lua.create_string(bytes).unwrap()));
    }
    for name in ["XMod", "CMod", "MMod", "CAMod", "WeirdSpeed"] {
        for colon in [false, true] {
            let empty: MultiValue = if colon {
                vec![Value::Table(foreign.clone())].into_iter().collect()
            } else {
                MultiValue::new()
            };
            assert_eq!(
                speed_outcome(&lua, &empty, name, true, false),
                speed_outcome(&lua, &empty, name, false, false)
            );
            for value in &values {
                for extras in [0, 1, 3, 64, 257] {
                    let mut args = empty.clone();
                    args.push_back(value.clone());
                    for index in 0..extras {
                        args.push_back(Value::Number(if index == 0 { 0.5 } else { 9.0 }));
                    }
                    for corrupt in [false, true] {
                        assert_eq!(
                            speed_outcome(&lua, &args, name, true, corrupt),
                            speed_outcome(&lua, &args, name, false, corrupt),
                            "{name}, colon={colon}, extras={extras}, {value:?}"
                        );
                    }
                }
            }
        }
    }
    // Reads must observe arbitrary mutations, including raw fields that bypass metamethods.
    let owner = lua.create_table().unwrap();
    let old = speed(&lua, &owner, "XMod", true);
    let new = speed(&lua, &owner, "XMod", false);
    for active in values {
        owner.raw_set("__songlua_speedmod_active", active).unwrap();
        owner.raw_set("__songlua_speedmod_xmod", "4.5").unwrap();
        let read = |f: &Function| {
            f.call::<Value>(&owner)
                .map(fingerprint)
                .map_err(|e| e.to_string())
        };
        assert_eq!(read(&old), read(&new));
    }
}

#[test]
fn option_music_rate_preserves_receiver_metatables_and_error_order() {
    let lua = Lua::new();
    for source in [
        "nil",
        "false",
        "-2.5",
        "' 2.5 '",
        "'NaN'",
        "'inf'",
        "{}",
        "string.char(255)",
    ] {
        for fail in [false, true] {
            let mut results = Vec::new();
            for old in [true, false] {
                let (own, method) = music(&lua, old);
                lua.globals().set("rate", method).unwrap();
                lua.globals().set("own", own).unwrap();
                lua.globals().set("fail", fail).unwrap();
                let script = format!(
                    r#"local seen=''; local t=setmetatable({{}}, {{
                    __index=function(_,k) seen=seen..'read:'..k..';';return 1.75 end,
                    __newindex=function(t,k,v) seen=seen..'write:'..k..';';rawset(t,k,v);if fail then error('rate write failed') end end}})
                    local ok,v=pcall(rate,t,{source},9,10); return ok, tostring(v),seen,tostring(rawget(t,'__songlua_music_rate')),own.__songlua_music_rate"#
                );
                results.push(
                    lua.load(&script)
                        .set_name("rate-errors")
                        .eval::<(bool, String, String, String, f32)>()
                        .unwrap(),
                );
                assert_eq!(
                    lua.load("return rate(),rate(nil,2),rate(2,3),rate(own,nil)")
                        .eval::<(f32, f32, f32, f32)>()
                        .unwrap(),
                    (1.0, 1.0, 1.0, 1.25)
                );
            }
            assert_eq!(results[0], results[1], "{source}, fail={fail}");
        }
    }
}

#[test]
fn option_numeric_calls_have_zero_warm_churn() {
    let lua = Lua::new();
    lua.gc_stop();
    let owner = lua.create_table().unwrap();
    let old = speed(&lua, &owner, "XMod", true);
    let new = speed(&lua, &owner, "XMod", false);
    for function in [&old, &new] {
        function.call::<Value>((&owner, 2.0, 0.5)).unwrap();
    }
    let (rate_owner, rate) = music(&lua, false);
    let args: MultiValue = std::iter::once(Value::Table(owner.clone()))
        .chain(std::iter::once(Value::Number(2.0)))
        .chain(std::iter::repeat_n(Value::Number(0.5), 256))
        .collect();
    for _ in 0..8 {
        new.call::<Value>(&args).unwrap();
        rate.call::<f32>((&rate_owner, 1.5)).unwrap();
    }
    crate::perf::assert_no_churn(|| {
        for _ in 0..64 {
            black_box(new.call::<Value>(&args).unwrap());
            black_box(new.call::<Value>(&owner).unwrap());
            black_box(rate.call::<f32>((&rate_owner, 1.5)).unwrap());
            black_box(rate.call::<f32>(&rate_owner).unwrap());
        }
    });
    crate::perf::assert_reduced_churn(
        || {
            for _ in 0..64 {
                black_box(old.call::<Value>(&args).unwrap());
            }
        },
        || {
            for _ in 0..64 {
                black_box(new.call::<Value>(&args).unwrap());
            }
        },
    );
}

#[test]
#[ignore = "manual release comparison against 0.5.1633"]
fn call_transfer_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for action in [
        "get",
        "set",
        "clear",
        "no_receiver",
        "set_64args",
        "set_257args",
    ] {
        for old in order {
            let lua = Lua::new();
            lua.gc_stop();
            let owner = lua.create_table().unwrap();
            let function = speed(&lua, &owner, "XMod", old);
            function.call::<Value>((&owner, 2.0, 0.5)).unwrap();
            let mut args = MultiValue::new();
            if action != "no_receiver" {
                args.push_back(Value::Table(owner));
            }
            if action == "clear" {
                args.push_back(Value::Nil);
            } else if action.starts_with("set") {
                args.push_back(Value::Number(2.0));
                args.push_back(Value::Number(0.5));
            }
            if action == "set_64args" || action == "set_257args" {
                args.extend(std::iter::repeat_n(
                    Value::Integer(99),
                    if action == "set_64args" { 61 } else { 254 },
                ));
            }
            crate::perf::measure_sampled(
                &format!("option/speed/{action}/{}", if old { "old" } else { "new" }),
                256,
                64,
                || {
                    for _ in 0..64 {
                        black_box(function.call::<Value>(&args).unwrap());
                    }
                },
            );
        }
    }
    for action in ["get", "set", "no_receiver", "set_257args"] {
        for old in order {
            let lua = Lua::new();
            lua.gc_stop();
            let (owner, function) = music(&lua, old);
            let mut args = MultiValue::new();
            if action != "no_receiver" {
                args.push_back(Value::Table(owner));
            }
            if action.starts_with("set") {
                args.push_back(Value::Number(1.5));
            }
            if action == "set_257args" {
                args.extend(std::iter::repeat_n(Value::Integer(99), 255));
            }
            crate::perf::measure_sampled(
                &format!("option/music/{action}/{}", if old { "old" } else { "new" }),
                256,
                64,
                || {
                    for _ in 0..64 {
                        black_box(function.call::<f32>(&args).unwrap());
                    }
                },
            );
        }
    }
    for action in ["speed", "music"] {
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "option/install/{action}/{}",
                    if old { "old" } else { "new" }
                ),
                96,
                1,
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    let owner = lua.create_table().unwrap();
                    (lua, owner)
                },
                |(lua, owner)| {
                    if action == "speed" {
                        black_box(speed(lua, owner, "XMod", old));
                    } else {
                        black_box(music(lua, old));
                    }
                },
            );
        }
    }
}
