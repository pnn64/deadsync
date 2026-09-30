use super::*;
use mlua::{FromLua, FromLuaMulti};
use std::hint::black_box;

#[path = "state_text_baseline.rs"]
mod baseline;

fn install(lua: &Lua, owner: &Table, old: bool) -> [Function; 2] {
    if old {
        baseline::set_state_string_getter(lua, owner, "GetText", "text").unwrap();
        baseline::set_state_string_setter(lua, owner, "SetText", "text").unwrap();
    } else {
        set_state_string_getter(lua, owner, "GetText", "text").unwrap();
        set_state_string_setter(lua, owner, "SetText", "text").unwrap();
    }
    ["GetText", "SetText"].map(|key| owner.get(key).unwrap())
}

fn bytes(function: &Function, args: &MultiValue) -> Result<Vec<u8>, String> {
    function
        .call::<mlua::LuaString>(args)
        .map(|v| v.as_bytes().to_vec())
        .map_err(|e| e.to_string())
}

#[test]
fn state_text_getters_preserve_live_values_coercions_and_exact_utf8_errors() {
    let lua = Lua::new();
    let owner = lua.create_table().unwrap();
    let old = install(&lua, &owner, true);
    let new = install(&lua, &owner, false);
    let callback = lua.create_function(|_, ()| Ok(())).unwrap();
    let mut values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(i64::MIN),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Table(lua.create_table().unwrap()),
        Value::Function(callback.clone()),
        Value::Thread(lua.create_thread(callback).unwrap()),
    ];
    for text in [
        b"".as_slice(),
        b"short",
        b"utf8\0\xc3\xa9",
        &[0xff, 0, 0xfe],
    ] {
        values.push(Value::String(lua.create_string(text).unwrap()));
    }
    values.push(Value::String(
        lua.create_string("long text ".repeat(1024)).unwrap(),
    ));
    for value in values {
        owner.raw_set("text", value.clone()).unwrap();
        for count in [0, 1, 2, 64, 257] {
            let args: MultiValue =
                std::iter::repeat_n(Value::Table(owner.clone()), count).collect();
            assert_eq!(
                bytes(&old[0], &args),
                bytes(&new[0], &args),
                "{value:?}, args={count}"
            );
        }
        // The safe conversion preserves the corresponding safe String contract.
        let expected = String::from_lua(value.clone(), &lua)
            .map(String::into_bytes)
            .map_err(|e| e.to_string());
        let actual = crate::state_text::StateText::from_lua(value, &lua)
            .map(|v| v.into_string(&lua).unwrap().as_bytes().to_vec())
            .map_err(|e| e.to_string());
        assert_eq!(expected, actual);
    }
}

#[test]
fn state_text_setters_preserve_method_shapes_defaults_and_safe_fallback() {
    let lua = Lua::new();
    let receiver = lua.create_table().unwrap();
    let callback = lua.create_function(|_, ()| Ok(())).unwrap();
    let values = [
        Value::Nil,
        Value::Integer(17),
        Value::Boolean(true),
        Value::Table(receiver.clone()),
        Value::Function(callback),
        Value::String(lua.create_string("utf8\0\u{e9}").unwrap()),
        Value::String(lua.create_string([0xff, 0, 0xfe]).unwrap()),
        Value::String(lua.create_string("x".repeat(4096)).unwrap()),
    ];
    for colon in [false, true] {
        for value in &values {
            for count in [0, 1, 2, 8, 64, 257] {
                let mut args = MultiValue::new();
                if colon {
                    args.push_back(Value::Table(receiver.clone()));
                }
                if count > 0 {
                    args.push_back(value.clone());
                    args.extend(std::iter::repeat_n(
                        Value::String(lua.create_string("ignored").unwrap()),
                        count - 1,
                    ));
                }
                let mut outcomes = Vec::new();
                for old in [true, false] {
                    let owner = lua.create_table().unwrap();
                    let functions = install(&lua, &owner, old);
                    lua.globals()
                        .set(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY, 0)
                        .unwrap();
                    let returned = functions[1].call::<Table>(&args).unwrap();
                    outcomes.push((
                        returned.to_pointer() == owner.to_pointer(),
                        owner
                            .get::<mlua::LuaString>("text")
                            .unwrap()
                            .as_bytes()
                            .to_vec(),
                        crate::runtime::song_lua_side_effect_count(&lua).unwrap(),
                    ));
                }
                assert_eq!(outcomes[0], outcomes[1]);
                let expected = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                let fallback =
                    crate::state_text::StateTextArgs::from_lua_multi(args, &lua).unwrap();
                assert_eq!(
                    fallback
                        .0
                        .map(|s| s.to_str().unwrap().to_string())
                        .unwrap_or_default(),
                    expected
                );
            }
        }
    }
}

#[test]
fn state_text_preserves_metatable_reads_writes_and_partial_side_effect_errors() {
    for stage in ["none", "read", "write", "count"] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            lua.globals().set("stage", "none").unwrap();
            let owner=lua.load(r#"seen='';return setmetatable({}, {
                __index=function(t,k) seen=seen..'read;';if stage=='read' then error('read failed') end;return 'fallback' end,
                __newindex=function(t,k,v) seen=seen..'write;';rawset(t,k,v);if stage=='write' then error('write failed') end end})"#).set_name("text-errors").eval::<Table>().unwrap();
            let functions = install(&lua, &owner, old);
            lua.globals().set("seen", "").unwrap();
            lua.globals().set("stage", stage).unwrap();
            let mt=lua.load("return {__newindex=function(t,k,v)seen=seen..'count;';rawset(t,k,v);if stage=='count' then error('count failed') end end}").set_name("text-errors").eval::<Table>().unwrap();
            let globals = lua.globals();
            globals.set_metatable(Some(mt)).unwrap();
            let read = functions[0].call::<String>(()).map_err(|e| e.to_string());
            let written = functions[1]
                .call::<Table>((&owner, "new\0text"))
                .map(|t| t.to_pointer() == owner.to_pointer())
                .map_err(|e| e.to_string());
            globals.set_metatable(None).unwrap();
            outcomes.push((
                read,
                written,
                globals.get::<String>("seen").unwrap(),
                owner.raw_get::<String>("text").unwrap(),
                globals
                    .raw_get::<Option<i64>>(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY)
                    .unwrap(),
            ));
        }
        assert_eq!(outcomes[0], outcomes[1], "stage={stage}");
        assert_eq!(outcomes[0].3, "new\0text");
        if stage == "count" {
            assert!(outcomes[0].1.is_err());
            assert_eq!(outcomes[0].4, Some(1));
        }
    }
}

#[test]
fn state_text_valid_strings_have_zero_warm_churn() {
    let lua = Lua::new();
    lua.gc_stop();
    let owner = lua.create_table().unwrap();
    let old = install(&lua, &owner, true);
    let new = install(&lua, &owner, false);
    for text in [
        "".to_string(),
        "short".to_string(),
        "utf8\0\u{e9}".to_string(),
        "x".repeat(4096),
    ] {
        let text = lua.create_string(text).unwrap();
        owner.raw_set("text", &text).unwrap();
        for _ in 0..8 {
            for functions in [&old, &new] {
                functions[1].call::<Table>((&owner, &text)).unwrap();
                functions[0].call::<mlua::LuaString>(&owner).unwrap();
            }
        }
        crate::perf::assert_no_churn(|| {
            for _ in 0..64 {
                black_box(new[1].call::<Table>((&owner, &text)).unwrap());
                black_box(
                    new[0]
                        .call::<mlua::LuaString>((&owner, Value::Nil, false))
                        .unwrap(),
                );
            }
        });
        crate::perf::assert_reduced_churn(
            || {
                for _ in 0..64 {
                    black_box(old[1].call::<Table>((&owner, &text)).unwrap());
                    black_box(old[0].call::<mlua::LuaString>(&owner).unwrap());
                }
            },
            || {
                for _ in 0..64 {
                    black_box(new[1].call::<Table>((&owner, &text)).unwrap());
                    black_box(new[0].call::<mlua::LuaString>(&owner).unwrap());
                }
            },
        );
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
    for (label, text) in [
        ("empty", "".to_string()),
        ("short", "Player Name".to_string()),
        ("long", "x".repeat(4096)),
    ] {
        for action in ["get", "set"] {
            for old in order {
                let lua = Lua::new();
                lua.gc_stop();
                let owner = lua.create_table().unwrap();
                let text = lua.create_string(&text).unwrap();
                owner.raw_set("text", &text).unwrap();
                let functions = install(&lua, &owner, old);
                crate::perf::measure_sampled(
                    &format!("text/{action}/{label}/{}", if old { "old" } else { "new" }),
                    if label == "long" { 64 } else { 256 },
                    64,
                    || {
                        for _ in 0..64 {
                            if action == "get" {
                                black_box(functions[0].call::<mlua::LuaString>(&owner).unwrap());
                            } else {
                                black_box(functions[1].call::<Table>((&owner, &text)).unwrap());
                            }
                        }
                    },
                );
            }
        }
    }
    for action in ["get_noargs", "get_numeric", "set_numeric", "set_257args"] {
        for old in order {
            let lua = Lua::new();
            lua.gc_stop();
            let owner = lua.create_table().unwrap();
            owner
                .raw_set(
                    "text",
                    if action == "get_numeric" {
                        Value::Integer(17)
                    } else {
                        Value::String(lua.create_string("stored").unwrap())
                    },
                )
                .unwrap();
            let functions = install(&lua, &owner, old);
            let args: MultiValue = std::iter::once(Value::Table(owner.clone()))
                .chain(std::iter::once(if action == "set_numeric" {
                    Value::Integer(17)
                } else {
                    Value::String(lua.create_string("updated").unwrap())
                }))
                .chain(std::iter::repeat_n(Value::Boolean(false), 255))
                .collect();
            crate::perf::measure_sampled(
                &format!("text/control/{action}/{}", if old { "old" } else { "new" }),
                128,
                64,
                || {
                    for _ in 0..64 {
                        match action {
                            "get_noargs" => {
                                black_box(functions[0].call::<mlua::LuaString>(()).unwrap());
                            }
                            "get_numeric" => {
                                black_box(functions[0].call::<mlua::LuaString>(&owner).unwrap());
                            }
                            "set_numeric" => {
                                black_box(functions[1].call::<Table>((&owner, 17)).unwrap());
                            }
                            _ => {
                                black_box(functions[1].call::<Table>(&args).unwrap());
                            }
                        }
                    }
                },
            );
        }
    }
    for old in order {
        let lua = Lua::new();
        lua.gc_stop();
        let owner = lua.create_table().unwrap();
        install(&lua, &owner, old);
        lua.globals().set("owner", owner).unwrap();
        let run=lua.load("return function()for i=1,64 do owner:SetText('updated');local text=owner:GetText() end end").eval::<Function>().unwrap();
        crate::perf::measure_sampled(
            &format!("text/lua_loop/{}", if old { "old" } else { "new" }),
            256,
            128,
            || run.call::<()>(()).unwrap(),
        );
        crate::perf::measure_sampled_with_setup(
            &format!("text/install/{}", if old { "old" } else { "new" }),
            96,
            2,
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
