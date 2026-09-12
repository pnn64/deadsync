use super::*;
use std::hint::black_box;

#[path = "command_transfer_baseline.rs"]
mod baseline;

fn call(
    lua: &Lua,
    actor: &Table,
    command: &Function,
    params: Option<Value>,
    old: bool,
) -> mlua::Result<()> {
    if old {
        baseline::call_actor_function(lua, actor, command, params)
    } else {
        call_actor_function(lua, actor, command, params)
    }
}

#[test]
fn lua_transfer_commands_preserve_argument_count_identity_results_and_errors() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let command = lua.load("return function(...) seen=table.pack(...); seen_dir=__songlua_script_dir; if fail then error('command failed') end return 42,nil,'discarded' end").eval::<Function>().unwrap();
    let globals = lua.globals();
    let table = Value::Table(lua.create_table().unwrap());
    let invalid = Value::String(lua.create_string([0xff]).unwrap());
    for dir in [
        Value::Nil,
        Value::String(lua.create_string("").unwrap()),
        Value::String(lua.create_string(" \t").unwrap()),
        Value::String(lua.create_string("song/éあ").unwrap()),
    ] {
        actor.raw_set("__songlua_script_dir", dir.clone()).unwrap();
        for params in [
            None,
            Some(Value::Nil),
            Some(Value::Boolean(false)),
            Some(Value::Integer(7)),
            Some(Value::Number(-0.0)),
            Some(table.clone()),
            Some(invalid.clone()),
        ] {
            for fail in [false, true] {
                let mut outcomes = Vec::new();
                for old in [true, false] {
                    globals.set("__songlua_script_dir", "parent").unwrap();
                    globals.set("fail", fail).unwrap();
                    let result = call(&lua, &actor, &command, params.clone(), old);
                    let seen: Table = globals.get("seen").unwrap();
                    assert_eq!(
                        seen.raw_get::<usize>("n").unwrap(),
                        if params.is_some() { 2 } else { 1 }
                    );
                    assert_eq!(
                        seen.raw_get::<Table>(1).unwrap().to_pointer(),
                        actor.to_pointer()
                    );
                    if let Some(expected) = &params {
                        let actual: Value = seen.raw_get(2).unwrap();
                        match (expected, actual) {
                            (Value::String(a), Value::String(b)) => {
                                assert_eq!(a.as_bytes().as_ref(), b.as_bytes().as_ref())
                            }
                            (Value::Number(a), Value::Number(b)) => {
                                assert_eq!(a.to_bits(), b.to_bits())
                            }
                            (a, b) => assert!(crate::values::lua_values_equal(a, &b)),
                        }
                    }
                    assert_eq!(
                        globals.get::<String>("__songlua_script_dir").unwrap(),
                        "parent"
                    );
                    assert_eq!(result.is_err(), fail);
                    outcomes.push((
                        result.map_err(|e| e.to_string()),
                        globals.get::<String>("seen_dir").unwrap(),
                    ));
                }
                assert_eq!(outcomes[0], outcomes[1]);
            }
        }
    }
    // Script-directory lookup must fail before invoking the command.
    for dir in [Value::Table(lua.create_table().unwrap()), invalid] {
        actor.raw_set("__songlua_script_dir", dir).unwrap();
        let mut errors = Vec::new();
        for old in [true, false] {
            globals.set("seen", Value::Nil).unwrap();
            errors.push(
                call(&lua, &actor, &command, None, old)
                    .unwrap_err()
                    .to_string(),
            );
            assert!(matches!(globals.get::<Value>("seen").unwrap(), Value::Nil));
        }
        assert_eq!(errors[0], errors[1]);
    }
}

#[test]
fn lua_transfer_nested_commands_restore_script_directory_after_success_and_error() {
    for fail in [false, true] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let outer = lua.create_table().unwrap();
            let inner = lua.create_table().unwrap();
            outer.set("__songlua_script_dir", "outer").unwrap();
            inner.set("__songlua_script_dir", "inner").unwrap();
            let inner_fn = lua.load("return function() trace[#trace+1]=__songlua_script_dir; if fail then error('nested failure') end end").eval::<Function>().unwrap();
            lua.globals()
                .set(
                    "invoke_inner",
                    lua.create_function(move |lua, ()| call(lua, &inner, &inner_fn, None, old))
                        .unwrap(),
                )
                .unwrap();
            lua.globals()
                .set("trace", lua.create_table().unwrap())
                .unwrap();
            lua.globals().set("fail", fail).unwrap();
            lua.globals().set("__songlua_script_dir", "parent").unwrap();
            let outer_fn = lua.load("return function() trace[#trace+1]=__songlua_script_dir; invoke_inner(); trace[#trace+1]=__songlua_script_dir end").eval::<Function>().unwrap();
            let result = call(&lua, &outer, &outer_fn, None, old);
            let trace: Vec<String> = lua
                .globals()
                .get::<Table>("trace")
                .unwrap()
                .sequence_values()
                .collect::<mlua::Result<_>>()
                .unwrap();
            assert_eq!(
                trace,
                if fail {
                    vec!["outer", "inner"]
                } else {
                    vec!["outer", "inner", "outer"]
                }
            );
            assert_eq!(
                lua.globals().get::<String>("__songlua_script_dir").unwrap(),
                "parent"
            );
            assert_eq!(result.is_err(), fail);
            outcomes.push((result.map_err(|e| e.to_string()), trace));
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[test]
fn lua_transfer_commands_have_no_argument_buffer_churn() {
    let lua = Lua::new();
    let command = lua
        .load("return function(...) return ... end")
        .eval::<Function>()
        .unwrap();
    let actor = lua.create_table().unwrap();
    lua.gc_stop();
    // Intern the lookup key while keeping the actor's Rust handle unique.
    actor.raw_get::<Value>("__songlua_script_dir").unwrap();
    crate::perf::assert_no_churn(|| {
        for params in [None, Some(Value::Nil), Some(Value::Number(1.5))] {
            call_actor_function(&lua, black_box(&actor), &command, params).unwrap();
        }
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_transfer_bench_commands() {
    for directory in [false, true] {
        for kind in ["none", "nil", "number", "table"] {
            let lua = Lua::new();
            let actor = lua.create_table().unwrap();
            if directory {
                actor
                    .raw_set("__songlua_script_dir", "Songs/Benchmark/")
                    .unwrap();
            }
            let command = lua.load("return function(self, ...) self.value=select(1,...); self.count=select('#',...); return self end").eval::<Function>().unwrap();
            let params = match kind {
                "none" => None,
                "nil" => Some(Value::Nil),
                "number" => Some(Value::Number(2.5)),
                _ => Some(Value::Table(lua.create_table().unwrap())),
            };
            call(&lua, &actor, &command, params.clone(), true).unwrap();
            let before = (
                actor.raw_get::<Value>("value").unwrap(),
                actor.raw_get::<usize>("count").unwrap(),
            );
            call(&lua, &actor, &command, params.clone(), false).unwrap();
            assert!(crate::values::lua_values_equal(
                &before.0,
                &actor.raw_get("value").unwrap()
            ));
            assert_eq!(before.1, actor.raw_get::<usize>("count").unwrap());
            lua.gc_stop();
            for old in order() {
                crate::perf::measure_sampled(
                    &format!(
                        "command_{kind}_dir_{directory}/{}",
                        if old { "old" } else { "new" }
                    ),
                    128,
                    64,
                    || {
                        for _ in 0..64 {
                            call(
                                black_box(&lua),
                                black_box(&actor),
                                &command,
                                black_box(params.clone()),
                                old,
                            )
                            .unwrap();
                        }
                        black_box(&actor);
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
