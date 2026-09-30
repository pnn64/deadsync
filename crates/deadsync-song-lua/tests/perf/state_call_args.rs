use super::*;
use std::hint::black_box;

#[path = "state_call_args_baseline.rs"]
mod baseline;

fn install(lua: &Lua, owner: &Table, old: bool) -> [Function; 4] {
    if old {
        baseline::set_state_number_getter(lua, owner, "GetNumber", "number").unwrap();
        baseline::set_state_number_setter(lua, owner, "SetNumber", "number").unwrap();
        baseline::set_state_bool_getter(lua, owner, "GetBool", "boolean").unwrap();
        baseline::set_state_bool_setter(lua, owner, "SetBool", "boolean").unwrap();
    } else {
        set_state_number_getter(lua, owner, "GetNumber", "number").unwrap();
        set_state_number_setter(lua, owner, "SetNumber", "number").unwrap();
        set_state_bool_getter(lua, owner, "GetBool", "boolean").unwrap();
        set_state_bool_setter(lua, owner, "SetBool", "boolean").unwrap();
    }
    ["GetNumber", "SetNumber", "GetBool", "SetBool"].map(|key| owner.get(key).unwrap())
}

fn fingerprint(value: Value) -> String {
    match value {
        Value::Number(v) => format!("number:{:x}", v.to_bits()),
        Value::String(v) => format!("string:{:?}", v.as_bytes().as_ref()),
        other => format!("{other:?}"),
    }
}

#[test]
fn state_calls_match_parent_shapes_coercions_and_live_captured_state() {
    let lua = Lua::new();
    let foreign = lua.create_table().unwrap();
    let callback = lua.create_function(|_, ()| Ok(())).unwrap();
    let mut values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(i64::MIN),
        Value::Number(-0.0),
        Value::Number(2.5),
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
        b"YES",
        b"off",
        b"invalid",
        &[0xff, 0, 0xfe],
    ] {
        values.push(Value::String(lua.create_string(bytes).unwrap()));
    }
    for index in 0..4 {
        for colon in [false, true] {
            for count in [0_usize, 1, 2, 8, 64, 257] {
                for value in &values {
                    let mut args = MultiValue::new();
                    if colon {
                        args.push_back(Value::Table(foreign.clone()));
                    }
                    if count > 0 {
                        args.push_back(value.clone());
                    }
                    args.extend(std::iter::repeat_n(
                        Value::Number(99.0),
                        count.saturating_sub(1),
                    ));
                    let mut outcomes = Vec::new();
                    for old in [true, false] {
                        let owner = lua.create_table().unwrap();
                        owner.set("number", 4.5).unwrap();
                        owner.set("boolean", true).unwrap();
                        lua.globals()
                            .set(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY, 0)
                            .unwrap();
                        let methods = install(&lua, &owner, old);
                        let returned = match methods[index].call::<Value>(&args) {
                            Ok(Value::Table(t)) => {
                                format!("owner:{}", t.to_pointer() == owner.to_pointer())
                            }
                            Ok(v) => fingerprint(v),
                            Err(e) => e.to_string(),
                        };
                        outcomes.push((
                            returned,
                            fingerprint(owner.get("number").unwrap()),
                            fingerprint(owner.get("boolean").unwrap()),
                            crate::runtime::song_lua_side_effect_count(&lua).unwrap(),
                        ));
                    }
                    assert_eq!(
                        outcomes[0], outcomes[1],
                        "method={index}, colon={colon}, count={count}, {value:?}"
                    );
                }
            }
        }
    }
    let owner = lua.create_table().unwrap();
    let old = install(&lua, &owner, true);
    let new = install(&lua, &owner, false);
    for value in values {
        for (index, key) in [(0, "number"), (2, "boolean")] {
            owner.raw_set(key, value.clone()).unwrap();
            let read = |f: &Function| {
                f.call::<Value>((&foreign, Value::Nil, false))
                    .map(fingerprint)
                    .map_err(|e| e.to_string())
            };
            assert_eq!(read(&old[index]), read(&new[index]));
        }
    }
}

#[test]
fn state_calls_preserve_metatables_reentry_and_side_effect_error_order() {
    for stage in ["none", "read", "write", "count"] {
        for boolean in [false, true] {
            let mut outcomes = Vec::new();
            for old in [true, false] {
                let lua = Lua::new();
                lua.globals().set("stage", stage).unwrap();
                lua.globals().set("boolean", boolean).unwrap();
                let owner=lua.load(r#"seen=''; return setmetatable({}, {
                    __index=function(t,k) seen=seen..'read:'..k..';';if stage=='read' then error('read failed') end; return boolean and true or 2.5 end,
                    __newindex=function(t,k,v) seen=seen..'write:'..k..';';rawset(t,k,v); if stage=='write' then error('write failed') end end})"#).set_name("state-errors").eval::<Table>().unwrap();
                // Installation also invokes __newindex; disable errors until methods exist.
                lua.globals().set("stage", "none").unwrap();
                let methods = install(&lua, &owner, old);
                lua.globals().set("seen", "").unwrap();
                lua.globals().set("stage", stage).unwrap();
                let globals = lua.globals();
                let mt=lua.load(r#"return {__newindex=function(t,k,v) seen=seen..'count;';rawset(t,k,v);if stage=='count' then error('count failed') end end}"#).set_name("state-errors").eval::<Table>().unwrap();
                globals.set_metatable(Some(mt)).unwrap();
                let index = if boolean { 2 } else { 0 };
                let read = methods[index]
                    .call::<Value>(())
                    .map(fingerprint)
                    .map_err(|e| e.to_string());
                let written = methods[index + 1]
                    .call::<Table>((
                        &owner,
                        if boolean {
                            Value::Boolean(false)
                        } else {
                            Value::Number(1.5)
                        },
                    ))
                    .map(|t| t.to_pointer() == owner.to_pointer())
                    .map_err(|e| e.to_string());
                globals.set_metatable(None).unwrap();
                outcomes.push((
                    read,
                    written,
                    globals.get::<String>("seen").unwrap(),
                    fingerprint(
                        owner
                            .raw_get(if boolean { "boolean" } else { "number" })
                            .unwrap(),
                    ),
                    globals
                        .raw_get::<Option<i64>>(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY)
                        .unwrap(),
                ));
            }
            assert_eq!(outcomes[0], outcomes[1], "stage={stage}, boolean={boolean}");
            if stage == "count" {
                assert!(outcomes[0].1.is_err());
                assert_eq!(outcomes[0].4, Some(1));
            }
        }
    }
    // A read metamethod may reenter a setter; it must still see the live owner.
    for old in [true, false] {
        let lua = Lua::new();
        let owner = lua.create_table().unwrap();
        install(&lua, &owner, old);
        lua.globals().set("owner", owner).unwrap();
        assert_eq!(lua.load("setmetatable(owner,{__index=function(t,k)t:SetNumber(7);return rawget(t,k) end});return owner:GetNumber(),owner:GetNumber()").eval::<(f32,f32)>().unwrap(),(7.0,7.0));
        assert_eq!(crate::runtime::song_lua_side_effect_count(&lua).unwrap(), 1);
    }
}

#[test]
fn state_calls_have_zero_warm_churn_even_with_many_ignored_arguments() {
    let lua = Lua::new();
    lua.gc_stop();
    let owner = lua.create_table().unwrap();
    owner.set("number", 1.5).unwrap();
    owner.set("boolean", true).unwrap();
    let old = install(&lua, &owner, true);
    let new = install(&lua, &owner, false);
    for count in [1, 2, 64, 257] {
        let args: MultiValue = std::iter::once(Value::Table(owner.clone()))
            .chain(std::iter::repeat_n(Value::Number(1.5), count - 1))
            .collect();
        for _ in 0..8 {
            for function in old.iter().chain(&new) {
                function.call::<Value>(&args).unwrap();
            }
        }
        crate::perf::assert_no_churn(|| {
            for _ in 0..32 {
                for function in &new {
                    black_box(function.call::<Value>(&args).unwrap());
                }
            }
        });
        crate::perf::assert_reduced_churn(
            || {
                for function in &old {
                    black_box(function.call::<Value>(&args).unwrap());
                }
            },
            || {
                for function in &new {
                    black_box(function.call::<Value>(&args).unwrap());
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
    for (index, label) in [
        (0, "get_number"),
        (1, "set_number"),
        (2, "get_bool"),
        (3, "set_bool"),
    ] {
        for count in [0, 2, 257] {
            for old in order {
                let lua = Lua::new();
                lua.gc_stop();
                let owner = lua.create_table().unwrap();
                owner.set("number", 1.5).unwrap();
                owner.set("boolean", true).unwrap();
                let functions = install(&lua, &owner, old);
                let mut args = MultiValue::new();
                if count > 0 {
                    args.push_back(Value::Table(owner));
                    args.push_back(if index < 2 {
                        Value::Number(1.5)
                    } else {
                        Value::Boolean(true)
                    });
                    args.extend(std::iter::repeat_n(Value::Integer(99), count - 2));
                }
                crate::perf::measure_sampled(
                    &format!(
                        "state/{label}/{count}args/{}",
                        if old { "old" } else { "new" }
                    ),
                    256,
                    64,
                    || {
                        for _ in 0..64 {
                            black_box(functions[index].call::<Value>(&args).unwrap());
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
        owner.set("number", 1.5).unwrap();
        owner.set("boolean", true).unwrap();
        install(&lua, &owner, old);
        lua.globals().set("owner", owner).unwrap();
        let run=lua.load("return function() for i=1,64 do owner:SetNumber(1.5);local n=owner:GetNumber();owner:SetBool(true);local b=owner:GetBool() end end").eval::<Function>().unwrap();
        crate::perf::measure_sampled(
            &format!("state/lua_loop/{}", if old { "old" } else { "new" }),
            256,
            256,
            || run.call::<()>(()).unwrap(),
        );
        crate::perf::measure_sampled_with_setup(
            &format!("state/install/{}", if old { "old" } else { "new" }),
            96,
            4,
            || {
                let lua = Lua::new();
                lua.gc_stop();
                let owner = lua.create_table().unwrap();
                (lua, owner)
            },
            |(lua, owner)| {
                black_box(install(lua, owner, old));
            },
        );
    }
}
