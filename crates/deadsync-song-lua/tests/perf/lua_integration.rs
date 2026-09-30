use mlua::{Function, Lua, MultiValue, Table, Value};
use std::hint::black_box;

#[path = "lua_integration_baseline.rs"]
mod baseline;

fn install(lua: &Lua, old: bool) {
    if old {
        baseline::install_cmd_helpers(lua).unwrap();
    } else {
        crate::host::install_cmd_helpers(lua).unwrap();
    }
}

fn command(lua: &Lua, name: Option<Value>, args: &MultiValue, old: bool) -> Function {
    install(lua, old);
    let builder: Function = lua.globals().get("cmd").unwrap();
    if let Some(name) = name {
        builder.call((name, args)).unwrap()
    } else {
        builder.call(()).unwrap()
    }
}

fn check_value(actual: Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.to_bits(), b.to_bits()),
        (Value::String(a), Value::String(b)) => assert_eq!(a.as_bytes(), b.as_bytes()),
        (Value::Function(a), Value::Function(b)) => assert_eq!(a.to_pointer(), b.to_pointer()),
        (Value::Thread(a), Value::Thread(b)) => assert_eq!(a.to_pointer(), b.to_pointer()),
        (Value::LightUserData(a), Value::LightUserData(b)) => assert_eq!(a.0, b.0),
        (a, b) => assert!(crate::values::lua_values_equal(&a, b), "{a:?} != {b:?}"),
    }
}

#[test]
fn lua_integration_commands_preserve_argument_counts_values_identity_and_dynamic_lookup() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    lua.globals().set("actor", actor.clone()).unwrap();
    let method = lua
        .load("return function(self,...) assert(self==actor); seen=table.pack(...); return false,nil,999 end")
        .eval::<Function>()
        .unwrap();
    let function = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let values = [
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(i64::MAX),
        Value::Number(-0.0),
        Value::String(lua.create_string("\u{e9}\0\u{65e5}").unwrap()),
        Value::String(lua.create_string([0xff, 0, 0xfe]).unwrap()),
        Value::Table(lua.create_table().unwrap()),
        Value::Function(function.clone()),
        Value::Thread(lua.create_thread(function).unwrap()),
        Value::LightUserData(mlua::LightUserData(std::ptr::null_mut())),
        Value::Number(f64::INFINITY),
    ];
    for count in [0, 1, 2, 4, 5, 8, 16, 64, 257] {
        let args = (0..count)
            .map(|index| values[index % values.len()].clone())
            .collect::<MultiValue>();
        for name in ["sink", "\u{e9}\0method"] {
            for old in [true, false] {
                actor.raw_set(name, false).unwrap();
                let command = command(
                    &lua,
                    Some(Value::String(lua.create_string(name).unwrap())),
                    &args,
                    old,
                );
                // Methods are resolved when called, after command construction.
                actor.raw_set(name, method.clone()).unwrap();
                let result: Table = command.call(&actor).unwrap();
                assert_eq!(result.to_pointer(), actor.to_pointer());
                let seen: Table = lua.globals().get("seen").unwrap();
                assert_eq!(seen.raw_get::<usize>("n").unwrap(), count);
                for (index, value) in args.iter().enumerate() {
                    check_value(seen.raw_get(index + 1).unwrap(), value);
                }
            }
        }
    }
    for name in [
        None,
        Some(Value::Nil),
        Some(Value::Boolean(false)),
        Some(Value::Integer(1)),
        Some(Value::String(lua.create_string([0xff]).unwrap())),
        Some(Value::String(lua.create_string("missing").unwrap())),
    ] {
        for old in [true, false] {
            let command = command(&lua, name.clone(), &MultiValue::new(), old);
            let result: Table = command.call(&actor).unwrap();
            assert_eq!(result.to_pointer(), actor.to_pointer());
        }
    }
}

#[test]
fn lua_integration_commands_preserve_reentrancy_partial_errors_and_method_lookup_errors() {
    let lua = Lua::new();
    lua.load(
        r#"root={label='root'}; other={label='other'}
        function sink(self,...)
            log[#log+1]=self.label..':'..select('#',...)..':'..tostring(select(1,...))
            self.written=(self.written or 0)+1
            if self==root and not inside then inside=true;active(other);inside=false end
            if fail then error('method failed after write') end
            return nil, false, 123
        end
        root.sink=sink;other.sink=sink"#,
    )
    .set_name("cmd-reentrant")
    .exec()
    .unwrap();
    let actor: Table = lua.globals().get("root").unwrap();
    let args = MultiValue::from_vec(vec![Value::Integer(12), Value::Nil]);
    for fail in [false, true] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            lua.load("log={};inside=false;root.written=0;other.written=0")
                .exec()
                .unwrap();
            lua.globals().set("fail", fail).unwrap();
            let command = command(
                &lua,
                Some(Value::String(lua.create_string("sink").unwrap())),
                &args,
                old,
            );
            lua.globals().set("active", command.clone()).unwrap();
            let error = command.call::<Table>(&actor).err().map(|e| e.to_string());
            outcomes.push((
                error,
                lua.load("return table.concat(log,'|'),root.written,other.written")
                    .eval::<(String, i32, i32)>()
                    .unwrap(),
            ));
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
    let bad = lua
        .load("return setmetatable({}, {__index=function() error('lookup failed') end})")
        .set_name("cmd-lookup")
        .eval::<Table>()
        .unwrap();
    let mut errors = Vec::new();
    for old in [true, false] {
        let command = command(
            &lua,
            Some(Value::String(lua.create_string("sink").unwrap())),
            &args,
            old,
        );
        errors.push(command.call::<Table>(&bad).unwrap_err().to_string());
    }
    assert_eq!(errors[0], errors[1]);
}

#[test]
fn lua_integration_warm_command_argument_transfer_has_zero_churn_at_all_arities() {
    let lua = Lua::new();
    lua.gc_stop();
    let actor = lua
        .load("return {sink=function(self,...) return nil end}")
        .eval::<Table>()
        .unwrap();
    for count in [0, 1, 4, 8, 64, 257] {
        let args = (0..count).map(Value::Integer).collect::<MultiValue>();
        let name = Value::String(lua.create_string("sink").unwrap());
        let old = command(&lua, Some(name.clone()), &args, true);
        let new = command(&lua, Some(name), &args, false);
        for _ in 0..8 {
            old.call::<Table>(&actor).unwrap();
            new.call::<Table>(&actor).unwrap();
        }
        crate::perf::assert_no_churn(|| {
            black_box(new.call::<Table>(&actor).unwrap());
        });
        crate::perf::assert_reduced_churn(
            || {
                black_box(old.call::<Table>(&actor).unwrap());
            },
            || {
                black_box(new.call::<Table>(&actor).unwrap());
            },
        );
    }
}

fn register(lua: &Lua, function: &Function, env: &Table, old: bool) -> mlua::Result<()> {
    if old {
        baseline::register_loader_env(lua, function, env)
    } else {
        crate::files::register_loader_env(lua, function, env)
    }
}

fn retarget(lua: &Lua, function: &Function, env: &Table, old: bool) -> mlua::Result<()> {
    if old {
        baseline::retarget_loader_env(lua, function, env)
    } else {
        crate::files::retarget_loader_env(lua, function, env)
    }
}

#[test]
fn lua_integration_loader_registry_keeps_pointer_keys_aliases_replacement_and_missing_cases() {
    let lua = Lua::new();
    let a = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let b = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let first = lua.create_table().unwrap();
    let second = lua.create_table().unwrap();
    let target = lua.create_table().unwrap();
    for old in [true, false] {
        lua.globals()
            .raw_set("__songlua_loader_envs", Value::Nil)
            .unwrap();
        first.raw_set("__songlua_env_target", Value::Nil).unwrap();
        second.raw_set("__songlua_env_target", Value::Nil).unwrap();
        retarget(&lua, &a, &target, old).unwrap();
        assert!(matches!(
            lua.globals()
                .raw_get::<Value>("__songlua_loader_envs")
                .unwrap(),
            Value::Nil
        ));
        register(&lua, &a, &first, old).unwrap();
        let envs: Table = lua.globals().get("__songlua_loader_envs").unwrap();
        let key = format!("{:p}", a.to_pointer());
        assert_eq!(
            envs.get::<Table>(key.clone()).unwrap().to_pointer(),
            first.to_pointer()
        );
        retarget(&lua, &b, &target, old).unwrap();
        assert_eq!(envs.pairs::<Value, Value>().count(), 1);
        register(&lua, &a, &second, old).unwrap();
        retarget(&lua, &a, &target, old).unwrap();
        assert_eq!(
            envs.get::<Table>(key).unwrap().to_pointer(),
            second.to_pointer()
        );
        assert_eq!(
            second
                .get::<Table>("__songlua_env_target")
                .unwrap()
                .to_pointer(),
            target.to_pointer()
        );
        assert!(matches!(
            first.raw_get::<Value>("__songlua_env_target").unwrap(),
            Value::Nil
        ));
        register(&lua, &b, &first, old).unwrap();
        assert_eq!(envs.pairs::<Value, Value>().count(), 2);
    }
}

#[test]
fn lua_integration_loader_registry_preserves_metamethod_keys_values_and_errors() {
    let lua = Lua::new();
    let function = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let env = lua.create_table().unwrap();
    let mut results = Vec::new();
    for old in [true, false] {
        lua.load("__songlua_loader_envs=setmetatable({}, {__newindex=function(self,key,value) seen_key=key;seen_value=value;error('register failed') end})")
            .set_name("loader-register").exec().unwrap();
        let error = register(&lua, &function, &env, old)
            .unwrap_err()
            .to_string();
        assert_eq!(
            lua.globals()
                .get::<Table>("seen_value")
                .unwrap()
                .to_pointer(),
            env.to_pointer()
        );
        results.push((error, lua.globals().get::<String>("seen_key").unwrap()));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].1, format!("{:p}", function.to_pointer()));
    for code in [
        "__songlua_loader_envs=setmetatable({}, {__index=function() error('registry lookup failed') end})",
        "__songlua_loader_envs={};bad=setmetatable({}, {__newindex=function() error('target write failed') end})",
        "__songlua_loader_envs=123",
    ] {
        let mut errors = Vec::new();
        for old in [true, false] {
            lua.load(code).set_name("loader-errors").exec().unwrap();
            if code.contains("bad=") {
                let bad: Table = lua.globals().get("bad").unwrap();
                lua.globals()
                    .get::<Table>("__songlua_loader_envs")
                    .unwrap()
                    .raw_set(format!("{:p}", function.to_pointer()), bad)
                    .unwrap();
            }
            errors.push(
                retarget(&lua, &function, &env, old)
                    .unwrap_err()
                    .to_string(),
            );
        }
        assert_eq!(errors[0], errors[1]);
    }
}

#[test]
fn lua_integration_warm_loader_registration_and_retargeting_have_zero_churn() {
    let lua = Lua::new();
    lua.gc_stop();
    let function = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let missing = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let env = lua.create_table().unwrap();
    let target = lua.create_table().unwrap();
    for old in [true, false] {
        for _ in 0..8 {
            register(&lua, &function, &env, old).unwrap();
            retarget(&lua, &function, &target, old).unwrap();
            retarget(&lua, &missing, &target, old).unwrap();
        }
    }
    for mode in [0, 1, 2] {
        let run = |old| {
            if mode == 0 {
                register(&lua, &function, &env, old)
            } else {
                retarget(
                    &lua,
                    if mode == 1 { &function } else { &missing },
                    &target,
                    old,
                )
            }
            .unwrap();
        };
        crate::perf::assert_no_churn(|| run(false));
        crate::perf::assert_reduced_churn(|| run(true), || run(false));
    }
}

fn json(lua: &Lua, value: serde_json::Value, old: bool) -> Value {
    if old {
        baseline::json_to_lua_value(lua, value).unwrap()
    } else {
        crate::json::json_to_lua_value(lua, value).unwrap()
    }
}

fn signature(value: Value) -> String {
    match value {
        Value::Table(table) => {
            let mut entries = table
                .pairs::<Value, Value>()
                .map(|pair| {
                    let (key, value) = pair.unwrap();
                    format!("{}:{}", signature(key), signature(value))
                })
                .collect::<Vec<_>>();
            entries.sort();
            format!("table#{}{{{}}}", table.raw_len(), entries.join(","))
        }
        Value::Nil => "nil".into(),
        Value::Boolean(value) => format!("bool:{value}"),
        Value::Integer(value) => format!("integer:{value}"),
        Value::Number(value) => format!("number:{:x}", value.to_bits()),
        Value::String(value) => format!("string:{:?}", value.as_bytes()),
        _ => panic!("unexpected JSON-converted value"),
    }
}

#[test]
fn lua_integration_json_conversion_preserves_scalar_types_nested_tables_nulls_and_independence() {
    let lua = Lua::new();
    for value in [
        serde_json::Value::Null,
        serde_json::json!(true),
        serde_json::json!(i64::MIN),
        serde_json::json!(u64::MAX),
        serde_json::json!(-0.0),
        serde_json::json!(1.25),
        serde_json::json!("\u{e9}\0\u{65e5}"),
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::json!([null,1,null,{"nested":[1,2,3],"missing":null},null]),
        serde_json::json!({"1":123,"":false,"\u{e9}\0":[true,null,4],"null":null,"child":{"a":1,"b":2}}),
    ] {
        let old = json(&lua, value.clone(), true);
        let new = json(&lua, value, false);
        assert_eq!(signature(old.clone()), signature(new.clone()));
        if let (Value::Table(a), Value::Table(b)) = (old, new) {
            assert_ne!(a.to_pointer(), b.to_pointer());
            a.raw_set("mutation", true).unwrap();
            assert!(matches!(
                b.raw_get::<Value>("mutation").unwrap(),
                Value::Nil
            ));
        }
    }
}

#[test]
fn lua_integration_json_sparse_lengths_match_parent_and_dense_tables_avoid_growth_reallocations() {
    let lua = Lua::new();
    for width in 0..=10 {
        for mask in 0..1_usize << width {
            let value = serde_json::Value::Array(
                (0..width)
                    .map(|i| {
                        if mask & (1 << i) == 0 {
                            serde_json::Value::Null
                        } else {
                            serde_json::json!(i + 1)
                        }
                    })
                    .collect(),
            );
            assert_eq!(
                signature(json(&lua, value.clone(), true)),
                signature(json(&lua, value, false)),
                "width={width} mask={mask}"
            );
        }
    }
    lua.gc_collect().unwrap();
    lua.gc_stop();
    for width in [1, 8, 32, 128, 512] {
        let value = serde_json::Value::Array((0..width).map(|i| serde_json::json!(i)).collect());
        // Warm mlua's reference pool before measuring the output table.
        black_box(json(&lua, value.clone(), false));
        crate::perf::assert_churn_budget(8, width * 128 + 1024, || {
            black_box(json(&lua, value, false));
        });
    }
}

fn json_fixture(count: usize, style: &str) -> serde_json::Value {
    if style == "object" || style == "null_object" {
        serde_json::Value::Object(
            (0..count)
                .map(|i| {
                    (
                        format!("key_{i}"),
                        if style == "null_object" && i % 2 == 0 {
                            serde_json::Value::Null
                        } else {
                            serde_json::json!(i)
                        },
                    )
                })
                .collect(),
        )
    } else {
        serde_json::Value::Array(
            (0..count)
                .map(|i| match style {
                    "sparse" if i % 3 == 1 => serde_json::Value::Null,
                    "nested" => {
                        serde_json::json!({"name":"item","position":[i,i+1,i+2],"enabled":true})
                    }
                    _ => serde_json::json!(i),
                })
                .collect(),
        )
    }
}

#[test]
#[ignore = "manual old/new Lua integration CPU, throughput and allocator benchmark"]
fn lua_integration_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    let lua = Lua::new();
    lua.gc_stop();
    let actor = lua
        .load("return {sink=function(self,...) return nil end}")
        .eval::<Table>()
        .unwrap();
    for count in [0, 1, 4, 8, 64, 257] {
        let args = (0..count).map(Value::Integer).collect::<MultiValue>();
        for old in order {
            let cmd = command(
                &lua,
                Some(Value::String(lua.create_string("sink").unwrap())),
                &args,
                old,
            );
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(&format!("command/call/{count}/{mode}"), 2048, 64, || {
                for _ in 0..64 {
                    black_box(cmd.call::<Table>(&actor).unwrap());
                }
            });
        }
    }
    for count in [0, 1, 8, 64] {
        let args = (0..count).map(Value::Integer).collect::<MultiValue>();
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled_with_setup(
                &format!("command/build/{count}/{mode}"),
                128,
                1,
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    install(&lua, old);
                    let builder = lua.globals().get::<Function>("cmd").unwrap();
                    let name = Value::String(lua.create_string("sink").unwrap());
                    (lua, builder, name, args.clone())
                },
                |(_, builder, name, args)| {
                    black_box(builder.call::<Function>((name.clone(), &*args)).unwrap());
                },
            );
        }
    }
    let function = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let missing = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let env = lua.create_table().unwrap();
    let target = lua.create_table().unwrap();
    for name in ["register", "retarget", "missing"] {
        for old in order {
            register(&lua, &function, &env, old).unwrap();
            retarget(&lua, &missing, &target, old).unwrap();
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(&format!("loader/{name}/{mode}"), 2048, 64, || {
                for _ in 0..64 {
                    if name == "register" {
                        register(&lua, &function, &env, old).unwrap();
                    } else {
                        retarget(
                            &lua,
                            if name == "retarget" {
                                &function
                            } else {
                                &missing
                            },
                            &target,
                            old,
                        )
                        .unwrap();
                    }
                }
            });
        }
    }
    drop(lua);
    for (count, style) in [
        (0, "dense"),
        (1, "dense"),
        (8, "dense"),
        (64, "dense"),
        (512, "dense"),
        (4096, "dense"),
        (64, "sparse"),
        (512, "sparse"),
        (8, "object"),
        (64, "object"),
        (512, "object"),
        (512, "null_object"),
        (64, "nested"),
    ] {
        let value = json_fixture(count, style);
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled_with_setup(
                &format!("json/{style}/{count}/{mode}"),
                if count >= 512 { 128 } else { 256 },
                count.max(1),
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    black_box(json(&lua, value.clone(), old));
                    (lua, value.clone())
                },
                |(lua, value)| {
                    black_box(json(lua, std::mem::take(value), old));
                },
            );
        }
    }
}
